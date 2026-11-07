//! **两条路径的坐标必须逐位相同**（第三轮工程债，`docs/06-verification.md` §30）。
//!
//! 背景（`docs/142` §5）：收敛层 `_scaffold::diffconv` 曾经自己算过一遍坐标
//! （修正文本、点序列、停止点、爆炸点），内核 `game::Match` 里另有一份。
//! 两份实现并存时，任何一次对拍分歧都要先猜"是驱动错了还是内核错了"，
//! 而且**真的猜错过**（同一批数据两条路径得出不同结论）。
//!
//! 现在坐标只在**一个**入口算：[`gw2_rebuild::graphwar_shot_calculator::settle::resolve_shot`]。
//! 本文件把"合并没改变任何一位"这件事变成可执行断言，比的是**两条独立代码路径**：
//!
//! | 路径 | 入口 | 位置 |
//! |---|---|---|
//! | A 内核 | `Match::fire_functions` / `Match::fire_batch` | `src/game.rs` |
//! | B 收敛层 | `ConvPredictor::plan` / `ConvPredictor::fire_batch` | `src/_scaffold/diffconv.rs` |
//!
//! 断言的四个中间结果（逐位 / 逐条，**不是**近似相等）：
//!
//! 1. **点序列**：长度 + 每个 `(x, y)` 的 `to_bits()`；
//! 2. **stop**：`(x, y)` 的 `to_bits()`；
//! 3. **killed**：`id` 序列 + 每个命中的 `(arc.to_bits(), index)`；
//! 4. **effect 序列**：两条路径产出的报文逐条比（`kind` + 去掉 tick 的字段），
//!    唯一的**已知差异**是 `EffectInfo(ExplosionEffect).pos_y` —— 内核那一路发的是
//!    **采样停点**，收敛层发的是**回代爆炸点**（`docs/06` §24 的 `Y`/`Y2` 轴，
//!    属于 `game.rs` 的改动范围，不在本任务内）。本文件把这条差异也钉住：
//!    两侧各自必须等于自己那条规则的输出，差一条都不行。
//!
//! 语料缺失时**跳过并打印原因**（仓库约定：缺资产不许静默通过）；
//! 合成用例两条路径恒可比，所以 CI 上永远不会"什么都没比"。

use gw2_rebuild::_scaffold::diffconv::{mode_of, ConvPredictor};
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Msg, Replay, ShotInput};
use gw2_rebuild::game::{Match, Submission};
use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::graphwar_shot_calculator::settle::{ShotGeometry, Target};
use gw2_rebuild::server::RoomState;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

fn room() -> RoomState {
    RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
    }
}

fn f64_bits(x: f64) -> u64 {
    x.to_bits()
}

/// 一条报文里"可比"的字段（tick 是墙钟，`effect_id` 是会话计数器 —— 两侧初值不同）。
fn comparable_fields(m: &Msg) -> Vec<(String, String)> {
    m.body
        .iter()
        .filter(|f| f.name != "tick" && f.name != "active_tick" && f.name != "effect_id")
        .map(|f| (f.name.clone(), f.val.text()))
        .collect()
}

fn parse_all(wire: &[String]) -> Vec<Msg> {
    wire.iter()
        .map(|w| Msg::parse_wire(w).unwrap_or_else(|e| panic!("我方 emitter 产出了非法 JSON: {w} ({e})")))
        .collect()
}

/// 逐位比两个 [`ShotGeometry`] 与该批内核产物（`ShotOutcome` 序列）。
///
/// 返回比较过的发数，便于**统计样本量**（"比了 0 发"必须能被发现）。
fn assert_geometry_identical(
    file: &str,
    batch: usize,
    geoms: &[ShotGeometry],
    kernel: &[gw2_rebuild::graphwar_shot_calculator::settle::ShotOutcome],
) -> usize {
    assert_eq!(geoms.len(), kernel.len(), "{file}#{batch}: 发数不同");
    for (i, (g, k)) in geoms.iter().zip(kernel.iter()).enumerate() {
        let ctx = format!("{file}#{batch} 发 {i}");
        // 1. 点序列
        assert_eq!(
            g.outcome.points.len(),
            k.points.len(),
            "{ctx}: 采样点数不同（收敛层 {} / 内核 {}）",
            g.outcome.points.len(),
            k.points.len()
        );
        for (j, (a, b)) in g.outcome.points.iter().zip(k.points.iter()).enumerate() {
            assert_eq!(f64_bits(a.0), f64_bits(b.0), "{ctx}: 第 {j} 点 x 不是逐位相同");
            assert_eq!(f64_bits(a.1), f64_bits(b.1), "{ctx}: 第 {j} 点 y 不是逐位相同");
        }
        // 2. stop / blocked
        assert_eq!(f64_bits(g.outcome.stop.0), f64_bits(k.stop.0), "{ctx}: stop.x 不同");
        assert_eq!(f64_bits(g.outcome.stop.1), f64_bits(k.stop.1), "{ctx}: stop.y 不同");
        assert_eq!(g.outcome.blocked, k.blocked, "{ctx}: blocked 不同");
        // 3. killed + 命中细节（同时回合制按弧长排死亡，弧长必须逐位）
        assert_eq!(g.outcome.killed, k.killed, "{ctx}: killed 序列不同");
        assert_eq!(g.outcome.hits.len(), k.hits.len(), "{ctx}: hits 条数不同");
        for (j, (a, b)) in g.outcome.hits.iter().zip(k.hits.iter()).enumerate() {
            assert_eq!(a.id, b.id, "{ctx}: 第 {j} 个命中的 id 不同");
            assert_eq!(a.index, b.index, "{ctx}: 第 {j} 个命中的采样点下标不同");
            assert_eq!(f64_bits(a.arc), f64_bits(b.arc), "{ctx}: 第 {j} 个命中的弧长不是逐位相同");
        }
    }
    geoms.len()
}

/// 两条路径的**报文**逐条比，返回 `EffectInfo(Explosion).pos_y` 不同的条数。
///
/// `explosions` 是每发的收敛层爆炸点（用来核对"差异只可能来自那一条规则"）。
fn assert_effect_sequence_identical(
    file: &str,
    batch: usize,
    ours: &[Msg],
    kernel: &[Msg],
    explosions: &[(f64, f64)],
    kernel_stops: &[(f64, f64)],
) -> (usize, usize) {
    assert_eq!(
        ours.len(),
        kernel.len(),
        "{file}#{batch}: 报文条数不同（收敛层 {} / 内核 {}）",
        ours.len(),
        kernel.len()
    );
    let mut compared = 0usize;
    let mut posy_diff = 0usize;
    let mut expl_seen = 0usize;
    for (i, (a, b)) in ours.iter().zip(kernel.iter()).enumerate() {
        let ctx = format!("{file}#{batch} 第 {i} 条");
        assert_eq!(a.kind, b.kind, "{ctx}: 种类不同");
        let fa = comparable_fields(a);
        let fb = comparable_fields(b);
        assert_eq!(
            fa.iter().map(|x| &x.0).collect::<Vec<_>>(),
            fb.iter().map(|x| &x.0).collect::<Vec<_>>(),
            "{ctx}: 字段名/顺序不同"
        );
        for ((na, va), (nb, vb)) in fa.iter().zip(fb.iter()) {
            assert_eq!(na, nb, "{ctx}: 字段名不同");
            if a.kind == "EffectInfo"
                && a.get_str("effect_type").as_deref() == Some("ExplosionEffect")
                && na == "pos_y"
            {
                // 已知差异：内核发采样停点，收敛层发回代爆炸点。
                let ex = explosions[expl_seen].1;
                assert_eq!(
                    a.get_f64("pos_y").map(f64_bits),
                    Some(f64_bits(ex)),
                    "{ctx}: 收敛层的 pos_y 必须等于 settle::explosion_point 的输出"
                );
                assert_eq!(
                    b.get_f64("pos_y").map(f64_bits),
                    Some(f64_bits(kernel_stops[expl_seen].1)),
                    "{ctx}: 内核的 pos_y 必须等于采样停点"
                );
                if va != vb {
                    posy_diff += 1;
                }
                continue;
            }
            assert_eq!(va, vb, "{ctx}: 字段 {na} 不同（收敛层 {va} / 内核 {vb}）");
        }
        if a.kind == "EffectInfo" && a.get_str("effect_type").as_deref() == Some("ExplosionEffect") {
            expl_seen += 1;
        }
        compared += 1;
    }
    assert_eq!(expl_seen, explosions.len(), "{file}#{batch}: 爆炸特效条数与发数不符");
    (compared, posy_diff)
}

/// 两份模型：内核 `Match` + 收敛层 `ConvPredictor`。
struct Pair {
    kernel: Match,
    conv: ConvPredictor,
}

fn new_pair(inputs: &Inputs, base_tick: i64) -> Pair {
    let soldiers: Vec<Target> = inputs
        .pos
        .iter()
        .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
        .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();
    let mut kernel = Match::new(
        inputs.room.clone(),
        inputs.obstacles.clone(),
        soldiers.clone(),
        (base_tick, 0),
    );
    // 出生登记：靶子 ⇒ SparkEffect，`bot_` 皮肤 ⇒ AshEffect（两侧都要喂）
    for id in inputs.targets.iter() {
        kernel.add_target(*id, 1);
    }
    for (id, g) in inputs.skins.iter() {
        kernel.note_skin_graphics(*id, g);
    }
    let mut conv = ConvPredictor::new(&inputs.obstacles, soldiers, Mode::Normal);
    for (id, g) in inputs.skins.iter() {
        conv.skin.insert(*id, g.clone());
    }
    Pair { kernel, conv }
}

fn subs_of(shots: &[ShotInput]) -> Vec<Submission<'_>> {
    shots
        .iter()
        .map(|s| Submission {
            owner_id: s.owner,
            function_text: &s.text,
            x0: s.x0,
            y0: s.y0,
            flip: s.flip,
        })
        .collect()
}

fn shot_inputs_of(shots: &[ShotInput]) -> Vec<ShotInput> {
    shots
        .iter()
        .map(|s| ShotInput {
            owner: s.owner,
            x0: s.x0,
            y0: s.y0,
            flip: s.flip,
            text: s.text.clone(),
        })
        .collect()
}

/// 驱动一批：先比几何（`fire_functions` 与 `plan` 都不改状态），再比报文。
///
/// 返回 `(比较的报文条数, 比了几何的发数, Explosion.pos_y 不同的条数)`。
fn drive_batch(pair: &mut Pair, file: &str, batch: usize, shots: &[ShotInput], tick: i64) -> (usize, usize, usize) {
    let inputs = shot_inputs_of(shots);
    let subs = subs_of(shots);
    let geoms = pair.conv.plan(&inputs);
    let kernel_outcomes = pair.kernel.fire_functions(&subs);
    let shots_n = assert_geometry_identical(file, batch, &geoms, &kernel_outcomes);

    let explosions: Vec<(f64, f64)> = geoms.iter().map(|g| g.explosion).collect();
    let stops: Vec<(f64, f64)> = geoms.iter().map(|g| g.outcome.stop).collect();

    pair.kernel.tick_base = (tick, 0);
    let (conv_wire, _) = pair.conv.fire_batch(&inputs, tick);
    let kernel_wire = pair.kernel.fire_batch(&subs, 0);
    let (msgs, posy) = assert_effect_sequence_identical(
        file,
        batch,
        &parse_all(&conv_wire),
        &parse_all(&kernel_wire),
        &explosions,
        &stops,
    );
    (msgs, shots_n, posy)
}

/// 回放里这一批用的采样模式（只看批之前的最后一个 `FunctionModeInfo`）。
fn is_diffeq(r: &Replay, upto: usize) -> bool {
    let mut diff = false;
    for m in &r.msgs[..upto] {
        if m.kind == "FunctionModeInfo" {
            diff = m.get_str("function_mode").as_deref() == Some("DiffEqFunction");
        }
    }
    diff
}

fn corpus_dir() -> Option<PathBuf> {
    let d = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    if d.is_dir() {
        Some(d)
    } else {
        eprintln!("跳过：回放语料目录不存在（{}）", d.display());
        None
    }
}

// ---------------------------------------------------------------------------
// 1. 合成用例（不依赖语料，CI 上恒跑）
// ---------------------------------------------------------------------------

/// 一发普通射击（有障碍、有死者）：两条路径的点序列 / stop / killed / effect 全同。
#[test]
fn synthetic_shot_is_bit_identical_across_the_two_paths() {
    let obstacles = [(5.0, 0.0, 1.0)];
    let mut inputs = Inputs {
        room: room(),
        obstacles: obstacles.to_vec(),
        pos: Default::default(),
        team: Default::default(),
        alive: Default::default(),
        targets: Vec::new(),
        skins: Vec::new(),
    };
    inputs.alive.insert(1, true);
    inputs.alive.insert(7, true);
    inputs.pos.insert(1, (0.0, 0.0));
    inputs.pos.insert(7, (3.0, 0.0));
    let mut pair = new_pair(&inputs, 100);
    let shots = [ShotInput {
        owner: 1,
        x0: 0.0,
        y0: 0.0,
        flip: false,
        text: "sin(x)".into(),
    }];
    let (msgs, shots_n, _posy) = drive_batch(&mut pair, "<synthetic>", 0, &shots, 100);
    assert_eq!(shots_n, 1);
    assert!(msgs >= 7, "单发批至少 7 条报文，实际 {msgs}");
}

/// DiffEq 局**故意不在此断言**（`game.rs` 的 `fire_functions` 恒 `Mode::Normal`，
/// 那是它自己的已知缺口，见 `docs/06` §30.7）。这里只钉住"收敛层真的按 Ode 走"。
#[test]
fn diffeq_mode_is_conv_only() {
    let p = ConvPredictor::new(&[], vec![Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 }], Mode::Ode);
    assert_eq!(p.mode, Mode::Ode);
    assert_eq!(mode_of("DiffEqFunction"), Mode::Ode);
}

// ---------------------------------------------------------------------------
// 2. 真实语料（缺失则跳过）
// ---------------------------------------------------------------------------

/// **语料上两条路径逐位一致**。
///
/// 只比 **Normal 局**的回放：`game::Match::fire_functions` 恒用 `Mode::Normal`，
/// 所以 DiffEq 局的两条路径本来就不是同一个函数（`docs/06` §30.7 的已知缺口），
/// 拿它们比等于拿两个不同的规格比。
///
/// 默认只取**前 60 份**（排序后）控制测试时长；`GW2_CONSISTENCY_FILES=0` 跑全量。
/// 实际比了多少份 / 多少批 / 多少发会打印出来，并且断言"不许比了个零"。
#[test]
fn geometry_and_effects_are_bit_identical_on_the_corpus() {
    let Some(dir) = corpus_dir() else { return };
    let limit: usize = std::env::var("GW2_CONSISTENCY_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut files_used = 0usize;
    let mut files_skipped_diffeq = 0usize;
    let mut files_skipped_unreadable = 0usize;
    let mut batches = 0usize;
    let mut shots = 0usize;
    let mut msgs = 0usize;
    let mut posy_diff = 0usize;

    for path in files.iter() {
        if limit != 0 && files_used >= limit {
            break;
        }
        let Ok(replay) = Replay::load(path) else {
            files_skipped_unreadable += 1;
            continue;
        };
        let bs = split_batches(&replay);
        if bs.is_empty() {
            files_skipped_unreadable += 1;
            continue;
        }
        // 整份都是 Normal 才比（DiffEq 局的规格两侧不同）
        if bs.iter().any(|b| is_diffeq(&replay, b.start)) {
            files_skipped_diffeq += 1;
            continue;
        }
        let inputs = Inputs::scan(&replay, bs[0].start);
        if inputs.pos.is_empty() {
            files_skipped_unreadable += 1;
            continue;
        }
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut pair = new_pair(&inputs, replay.msgs[bs[0].start].tick);
        pair.conv.mode = mode_of(&inputs.room.function_mode);
        files_used += 1;

        for (idx, b) in bs.iter().enumerate() {
            if b.shots.is_empty() {
                continue;
            }
            let tick = replay.msgs[b.start].tick;
            let (m, s, p) = drive_batch(&mut pair, &name, idx, &b.shots, tick);
            msgs += m;
            shots += s;
            batches += 1;
            posy_diff += p;
        }
    }

    eprintln!(
        "一致性样本：回放 {files_used} 份 / 结算批 {batches} / 发 {shots} / 报文 {msgs}；\
         跳过 DiffEq {files_skipped_diffeq} 份、读不了 {files_skipped_unreadable} 份；\
         Explosion.pos_y 已知差异 {posy_diff} 条"
    );
    assert!(files_used > 0, "语料目录在（{}）却一份 Normal 局都没比到", dir.display());
    assert!(batches > 0 && shots > 0, "比了 0 批/0 发 —— 断言等于没写");
}
