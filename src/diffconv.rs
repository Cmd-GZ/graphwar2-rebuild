//! 回放消息级对拍的**收敛**层（`docs/142-diff-converge.md`）。
//!
//! `docs/136` 建的 `replaydiff` 是**口径 + 测量**：它把回放切成结算批、用
//! `game::Match::fire` 产报文、逐字节比。它量出了旧基线（完全一致 0/400），
//! 但 `fire` 有两条写死的假设让对拍**永远**过不去：
//!
//! | 写死的地方 | 真值 | 后果 |
//! |---|---|---|
//! | `fire` 恒用 `path::Mode::Normal` | `FunctionModeInfo = DiffEqFunction` 的批必须走 `Mode::Ode` | 550 发的几何整条错 |
//! | `shot::corrected_text` 恒加后缀 | **Normal** 加 `" ± c.ddd"`，**DiffEq 不加**（`corrected_function == function`，550/550 零例外） | 550 发全错 |
//!
//! 本模块**不改内核**（`game.rs` / `settle.rs` / `protocol.rs` 一字未动）：它自己按
//! `fire` 的顺序拼报文，只是把上面两条换成回放给的真值。**报文顺序、字段名、
//! 浮点写法全部复用 `protocol::emit_*`**，所以"逐字节"这条口径没有变松。
//!
//! 为什么另起一份而不是给 `Match::fire` 加参数：`fire` 的报文序列已经被
//! `tests/replay_diff.rs` 与端到端测试钉住，而同一时刻有别人在改 `game.rs` 的
//! 同时回合制事务化（`docs/136` §6 第 3 条）。把它做成**新增的、可丢弃的实验层**，
//! 结论成立之后再合并回 `fire`，主线不会被实验污染。
//!
//! 证据档位与仓库约定一致：结论标 **观测**（能指到回放里那一条报文）/
//! **逐位验证**（`tests/diff_conv.rs` 覆盖）/ **推断** / **未知**。

use crate::path::Mode;
use crate::plane::empty_mask;
use crate::replaydiff::{
    compare_batch, split_batches, BatchVerdict, Inputs, Msg, Policy, Replay, StateMask,
};
use crate::settle::{resolve, Target};

/// `QueueDestroyObstacle` → `DestroyObstacle` 的延迟（与 `game.rs` 同一个常量）。
pub use crate::game::DESTROY_DELAY_TICKS;

/// 回放里 `FunctionModeInfo` 的字符串 → 采样模式。
///
/// `NormalFunction` ⇒ `y = f(x)`；`DiffEqFunction` ⇒ `dy/dx = f(x, y)`（RK4）。
/// 未知值按 `Normal`（`docs/136` §3 的语料里只有这两个值，`docs/78`）。
pub fn mode_of(function_mode: &str) -> Mode {
    if function_mode == "DiffEqFunction" {
        Mode::Ode
    } else {
        Mode::Normal
    }
}

/// 修正文本的**唯一**规则（逐发真值见 `docs/142` §3.2）。
///
/// * `Mode::Ode`（DiffEq 局）：`corrected_function == function`，**不加后缀**
///   —— 全语料 550/550 零例外（回放实读）。
/// * `Mode::Normal`：`to_text(parse(function)) + " ± c.ddd"`。736/740 是
///   `function + suffix` 的字面拼接，剩下 4 条是空函数（`" + 3.333"`）；
///   另有 11 条函数**括号没闭合**，真值补了右括号（例：
///   `5/155sin(4exp(4*6cos(` → `5/155sin(4exp(4*6cos())) + 6.055`），
///   所以先过一遍 [`crate::function::to_text`]。
pub fn corrected_function(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode) -> String {
    match mode {
        Mode::Ode => text.to_string(),
        Mode::Normal => {
            let toks = crate::function::parse(text);
            format!(
                "{}{}{}",
                text,
                ")".repeat(unbalanced_brackets(text)),
                crate::shot::correction_text(&toks, x0, y0, flip)
            )
        }
    }
}

/// 字符串里没闭合的 `(` 个数（`(` 减 `)`, 下限 0）。
///
/// 真游戏发的是 `to_text(parse(function)) + 后缀`，而它的 `to_text` **原样保留**
/// 输入文本（740 条 Normal 里 736 条是 `function + 后缀` 的字面拼接），唯一的差别
/// 是**没闭合的括号会被补齐**（11 条，例 `5/155sin(4exp(4*6cos(` →
/// `5/155sin(4exp(4*6cos()))`）。本项目的 `function::to_text` 会重新加括号
/// （`to_text(parse("(40)*(1/(...))")) != "(40)*(1/(...))"`），所以不能直接用它。
pub fn unbalanced_brackets(text: &str) -> usize {
    let mut depth: i64 = 0;
    for ch in text.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            _ => {}
        }
    }
    depth.max(0) as usize
}

/// 爆炸点：**普通模式下它不是采样器的停点**。
///
/// 观测（739 发 Normal，`docs/142` §4.2）：把回放的 `FunctionPoints.end_x`
/// 代回**回放自己那条 `corrected_function` 文本**求值，得到的 y 与回放的
/// `EffectInfo(Explosion).pos_y` **逐位相同 529/739**、1~2 ulp 内 92 发；
/// 而我们采样器的停点 y 只有 **33/739**。⇒ 真游戏的爆炸点是"拿印刷版修正函数
/// 文本在 `end_x` 处求一遍"，不是采样器的最后一个采样点。
///
/// 为什么 `±x`：`flip` 时采样器内部用 `-x`，点序列里的 x 是取反回来的
/// （`path::calculate` 的 `px = if flip { -mid } else { mid }`），所以回代也要取反。
///
/// DiffEq（`Mode::Ode`）不走这条：那时 y 不是一个纯函数值，回代无意义，
/// 实测停点自己就是逐位相同的（376/550）。
pub fn explosion_point(corrected: &str, stop: (f64, f64), flip: bool, mode: Mode) -> (f64, f64) {
    if mode != Mode::Normal {
        return stop;
    }
    let toks = crate::function::parse(corrected);
    if toks.is_empty() {
        return stop;
    }
    let x = if flip { -stop.0 } else { stop.0 };
    let y = crate::function::eval(&toks, x, 0.0, 0.0);
    if y.is_nan() {
        return stop;
    }
    (stop.0, y)
}

/// 我方的一发预测（顺便留下几何量给归因用）。
#[derive(Debug, Clone)]
pub struct ShotOutcome {
    pub owner: i64,
    pub function: String,
    /// 修正文本（我方）
    pub corrected: String,
    pub stop: (f64, f64),
    pub blocked: bool,
    pub killed: Vec<i64>,
}

/// **模式感知**的预测器：与 `game::Match::fire` 同序同格式，但采样模式取自回放。
///
/// 状态自带（掩膜、士兵表、待删除障碍），不从回放里逐步重建——重建等于把答案
/// 抄进题目（`replaydiff::Predictor` 的注释是同一条理由）。
pub struct ConvPredictor {
    mask: Vec<u8>,
    pub soldiers: Vec<Target>,
    pub mode: Mode,
    pub next_effect_id: i64,
    pending_destroy: Vec<(i64, String)>,
    /// 每个士兵的皮肤（`SkinGraphicsInfo.graphics_str`）——**决定死亡特效的种类**。
    pub skin: std::collections::BTreeMap<i64, String>,
}

/// 死亡特效的种类由**皮肤的类别**决定（全语料 936 条逐条核对，见 `docs/142` §4.3）：
///
/// | 皮肤 | 特效 | 条数 |
/// |---|---|---|
/// | `target`（靶子） | `SparkEffect` | 54 / 54 |
/// | `bot_0`..`bot_5`（电脑玩家） | `AshEffect` | 363 / 368 |
/// | 人类皮肤（`yellow`/`green`/`face_paint_2`…） | `DeathEffect` | 514 / 514 |
///
/// 这是本轮**对拍才发现**的一条：`docs/136` 把它记成"我方没有 Ash/Spark 发射器"，
/// 其实不是缺发射器，是**同一次死亡按皮肤换特效类型**。
pub fn death_effect_type(skin: Option<&str>) -> &'static str {
    match skin {
        Some("target") => "SparkEffect",
        Some(s) if s.starts_with("bot_") => "AshEffect",
        _ => "DeathEffect",
    }
}

impl ConvPredictor {
    pub fn new(obstacles: &[(f64, f64, f64)], soldiers: Vec<Target>, mode: Mode) -> Self {
        let mut mask = empty_mask();
        for (x, y, r) in obstacles {
            crate::plane::rasterize(&mut mask, *x, *y, *r, 0x00);
        }
        ConvPredictor {
            mask,
            soldiers,
            mode,
            next_effect_id: 236,
            pending_destroy: Vec::new(),
            skin: std::collections::BTreeMap::new(),
        }
    }

    /// 与 `Match::fire` 一模一样的一条报文流（顺序见 `docs/43`），只有
    /// 采样模式与修正文本换成回放给的规则。
    pub fn fire(
        &mut self,
        owner_id: i64,
        function_text: &str,
        x0: f64,
        y0: f64,
        flip: bool,
        now_tick: i64,
    ) -> (Vec<String>, ShotOutcome) {
        let others: Vec<Target> = self
            .soldiers
            .iter()
            .filter(|t| !(t.id == owner_id) && !((t.x - x0).abs() < 1e-12 && (t.y - y0).abs() < 1e-12))
            .copied()
            .collect();

        let outcome = resolve(
            function_text,
            x0,
            y0,
            crate::SERVER_STEP,
            flip,
            self.mode,
            &self.mask,
            &others,
        );
        let corrected = corrected_function(function_text, x0, y0, flip, self.mode);

        let mut msgs = vec![
            crate::protocol::emit_game_state("CalculatingResults"),
            crate::protocol::emit_function_info(owner_id, function_text, Some(&corrected)),
            crate::protocol::emit_function_points(owner_id, x0, y0, outcome.stop.0),
            crate::protocol::emit_game_state("SynchronizingResults"),
        ];

        for id in &outcome.killed {
            msgs.push(crate::protocol::emit_life_queued_death(*id, now_tick));
            if let Some(t) = self.soldiers.iter().find(|t| t.id == *id) {
                self.next_effect_id += 1;
                let et = death_effect_type(self.skin.get(id).map(|s| s.as_str()));
                msgs.push(crate::protocol::emit_effect_info(
                    self.next_effect_id,
                    t.x,
                    t.y,
                    et,
                    now_tick,
                ));
            }
        }
        if !outcome.killed.is_empty() {
            self.soldiers.retain(|t| !outcome.killed.contains(&t.id));
        }

        if outcome.blocked {
            msgs.push(format!(
                r#"{{"QueueDestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5,"tick":{}}}}}"#,
                outcome.stop.0, outcome.stop.1, now_tick
            ));
            crate::plane::rasterize(&mut self.mask, outcome.stop.0, outcome.stop.1, 0.5, 0xFF);
            self.pending_destroy.push((
                now_tick + DESTROY_DELAY_TICKS,
                format!(
                    r#"{{"DestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5}}}}"#,
                    outcome.stop.0, outcome.stop.1
                ),
            ));
        }

        msgs.push(crate::protocol::emit_function_active(owner_id, now_tick));
        self.next_effect_id += 1;
        let (ex, ey) = explosion_point(&corrected, outcome.stop, flip, self.mode);
        msgs.push(crate::protocol::emit_effect_info(
            self.next_effect_id,
            ex,
            ey,
            "ExplosionEffect",
            now_tick,
        ));
        msgs.push(crate::protocol::emit_game_state("ExecutingResults"));

        (
            msgs,
            ShotOutcome {
                owner: owner_id,
                function: function_text.to_string(),
                corrected,
                stop: outcome.stop,
                blocked: outcome.blocked,
                killed: outcome.killed,
            },
        )
    }

    /// 到期的 `DestroyObstacle`（与 `Match::tick` 同义）。
    pub fn tick(&mut self, now_tick: i64) -> Vec<String> {
        let mut due = Vec::new();
        let mut i = 0;
        while i < self.pending_destroy.len() {
            if self.pending_destroy[i].0 <= now_tick {
                due.push(self.pending_destroy.remove(i).1);
            } else {
                i += 1;
            }
        }
        due
    }
}

// ===========================================================================
// 逐发几何（`docs/142` §4 的归因表就是这么来的）
// ===========================================================================

/// 一发"我方 vs 回放"的几何对照。
#[derive(Debug, Clone, Default)]
pub struct ShotGeom {
    pub file: String,
    pub batch: usize,
    pub shots_in_batch: usize,
    pub owner: i64,
    pub mode: &'static str,
    pub turn_mode: String,
    pub function: String,
    /// 我方停点
    pub stop_x: f64,
    pub stop_y: f64,
    pub blocked: bool,
    /// 回放声明
    pub end_x: f64,
    pub expl_x: f64,
    pub expl_y: f64,
    /// 我方采样点数 / 最后三个点（查"爆炸点是不是上一个点"用）
    pub n_points: usize,
    pub tail: Vec<(f64, f64)>,
    /// 我方判定的死者 / 回放窗口内的死效条数
    pub killed: Vec<i64>,
    pub obs_death_effects: usize,
    /// 回放在本批**之后**到下一批开始之间声明的死亡（`LifeInfo{alive:false}`）
    pub obs_died: Vec<i64>,
    /// 我方修正文本 vs 回放修正文本
    pub corrected_ours: String,
    pub corrected_obs: String,
    pub flip: bool,
    /// 假设 H1：爆炸点 = `eval(parse(回放的 corrected_function), ±end_x)`
    /// （客户端拿**印刷版**修正文本重算，常数只有 3 位小数）
    pub h1_y: f64,
    /// 假设 H2：把回放的 `corrected_function` **整条重算**一次，取最后一个点
    /// （客户端拿印刷版文本重放 → 常数只有 3 位小数）
    pub h2: (f64, f64),
    /// H2 的点数（用来和 `FunctionPoints` 的点数比）
    pub h2_n: usize,
    /// 我方步长路径的点数
    pub our_n: usize,
}

fn f64_eq_bits(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
}

/// 跑一份回放的**几何探针**：每一发一行，供离线归因。
///
/// 与 `replaydiff::run_file` 的区别只有一条：采样模式跟着回放的
/// `FunctionModeInfo` 走，而不是恒 `Normal`。
pub fn probe_file(path: &std::path::Path) -> Vec<ShotGeom> {
    let Ok(replay) = Replay::load(path) else {
        return Vec::new();
    };
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let batches = split_batches(&replay);
    if batches.is_empty() {
        return Vec::new();
    }
    let inputs = Inputs::scan(&replay, batches[0].start);
    let mut mode = mode_of(&inputs.room.function_mode);
    let mut turn_mode = inputs.room.turn_mode.clone();
    let mut alive: std::collections::BTreeSet<i64> = inputs
        .alive
        .iter()
        .filter(|(_, v)| **v)
        .map(|(k, _)| *k)
        .collect();
    let mut pos: std::collections::BTreeMap<i64, (f64, f64)> = inputs.pos.clone();
    let mut mask = StateMask::new();
    mask.apply(&replay.msgs[..batches[0].start]);
    let mut cursor = batches[0].start;
    let mut out = Vec::new();

    for (idx, b) in batches.iter().enumerate() {
        for m in &replay.msgs[cursor..b.start] {
            match m.kind.as_str() {
                "FunctionModeInfo" => {
                    if let Some(s) = m.get_str("function_mode") {
                        mode = mode_of(&s);
                    }
                }
                "TurnModeInfo" => {
                    if let Some(s) = m.get_str("turn_mode") {
                        turn_mode = s;
                    }
                }
                "LifeInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        if matches!(m.get("alive").map(|v| v.kind.clone()), Some(crate::replaydiff::wire::Kind::Bool(false))) {
                            alive.remove(&id);
                        } else {
                            alive.insert(id);
                        }
                    }
                }
                "EntityRemoved" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        alive.remove(&id);
                        pos.remove(&id);
                    }
                }
                "PosInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        pos.insert(
                            id,
                            (m.get_f64("pos_x").unwrap_or(0.0), m.get_f64("pos_y").unwrap_or(0.0)),
                        );
                        alive.insert(id);
                    }
                }
                _ => {}
            }
        }
        mask.apply(&replay.msgs[cursor..b.start]);
        cursor = b.start;
        if b.shots.is_empty() {
            continue;
        }
        let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();
        let obs_end_x: std::collections::BTreeMap<i64, f64> = observed
            .iter()
            .filter(|m| m.kind == "FunctionPoints")
            .filter_map(|m| Some((m.get_i64("owner_id")?, m.get_f64("end_x")?)))
            .collect();
        let mut obs_expl: std::collections::BTreeMap<i64, (f64, f64)> = Default::default();
        {
            let mut pending: Option<i64> = None;
            for m in &observed {
                if m.kind == "FunctionActive" {
                    pending = m.get_i64("owner_id");
                } else if m.kind == "EffectInfo"
                    && m.get_str("effect_type").as_deref() == Some("ExplosionEffect")
                {
                    if let Some(o) = pending.take() {
                        obs_expl.insert(
                            o,
                            (
                                m.get_f64("pos_x").unwrap_or(f64::NAN),
                                m.get_f64("pos_y").unwrap_or(f64::NAN),
                            ),
                        );
                    }
                }
            }
        }
        // 本批之后的延迟段（到下一批 `CalculatingResults` 为止）里的死亡
        let obs_died: Vec<i64> = {
            let mut v = Vec::new();
            let mut j = b.end + 1;
            while j < replay.msgs.len() {
                let m = &replay.msgs[j];
                if m.kind == "GameStateInfo" && m.get_str("game_state").as_deref() == Some("CalculatingResults") {
                    break;
                }
                if m.kind == "LifeInfo"
                    && matches!(m.get("alive").map(|x| x.kind.clone()), Some(crate::replaydiff::wire::Kind::Bool(false)))
                {
                    if let Some(id) = m.get_i64("entity_id") {
                        if !v.contains(&id) {
                            v.push(id);
                        }
                    }
                }
                j += 1;
            }
            v
        };
        let obs_death_effects = observed
            .iter()
            .filter(|m| m.kind == "EffectInfo")
            .filter(|m| m.get_str("effect_type").as_deref() == Some("DeathEffect"))
            .count();
        // 修正文本按**出现顺序**与 `FunctionPoints` 配对（同一 owner 在一批里可能开两发，
        // 按 owner 建表会把两条挤成一条 —— 那是本轮实测过的错）。
        let obs_corrected: Vec<String> = observed
            .iter()
            .filter(|m| m.kind == "FunctionInfo")
            .map(|m| m.get_str("corrected_function").unwrap_or_default())
            .collect();

        for (shot_i, shot) in b.shots.iter().enumerate() {
            let _ = shot_i;
            let targets: Vec<Target> = alive
                .iter()
                .filter(|id| **id != shot.owner)
                .filter_map(|id| pos.get(id).map(|(x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 }))
                .collect();
            let r = resolve(
                &shot.text,
                shot.x0,
                shot.y0,
                crate::SERVER_STEP,
                shot.flip,
                mode,
                mask.mask(),
                &targets,
            );
            let pts = crate::shot::shot_points(&shot.text, shot.x0, shot.y0, crate::SERVER_STEP, shot.flip, mode, true);
            let tail: Vec<(f64, f64)> = pts.iter().rev().take(4).rev().copied().collect();
            let n_points = pts.len();
            out.push(ShotGeom {
                file: name.clone(),
                batch: idx,
                shots_in_batch: b.shots.len(),
                owner: shot.owner,
                mode: if mode == Mode::Ode { "DiffEq" } else { "Normal" },
                turn_mode: turn_mode.clone(),
                function: shot.text.clone(),
                stop_x: r.stop.0,
                stop_y: r.stop.1,
                blocked: r.blocked,
                end_x: obs_end_x.get(&shot.owner).copied().unwrap_or(f64::NAN),
                expl_x: obs_expl.get(&shot.owner).map(|v| v.0).unwrap_or(f64::NAN),
                expl_y: obs_expl.get(&shot.owner).map(|v| v.1).unwrap_or(f64::NAN),
                n_points,
                tail,
                killed: r.killed.clone(),
                obs_death_effects,
                obs_died: obs_died.clone(),
                corrected_ours: corrected_function(&shot.text, shot.x0, shot.y0, shot.flip, mode),
                corrected_obs: obs_corrected.get(shot_i).cloned().unwrap_or_default(),
                flip: shot.flip,
                h1_y: {
                    let t = obs_corrected.get(shot_i).cloned().unwrap_or_default();
                    let xe = obs_end_x.get(&shot.owner).copied().unwrap_or(f64::NAN);
                    if t.is_empty() {
                        f64::NAN
                    } else {
                        let toks = crate::function::parse(&t);
                        crate::function::eval(&toks, if shot.flip { -xe } else { xe }, 0.0, 0.0)
                    }
                },
                h2: {
                    let t = obs_corrected.get(shot_i).cloned().unwrap_or_default();
                    let pts = crate::shot::shot_points(&t, shot.x0, shot.y0, crate::SERVER_STEP, shot.flip, mode, true);
                    let last = pts.last().copied().unwrap_or((f64::NAN, f64::NAN));
                    (last.0, last.1)
                },
                h2_n: {
                    let t = obs_corrected.get(shot_i).cloned().unwrap_or_default();
                    crate::shot::shot_points(&t, shot.x0, shot.y0, crate::SERVER_STEP, shot.flip, mode, true).len()
                },
                our_n: crate::shot::shot_points(&shot.text, shot.x0, shot.y0, crate::SERVER_STEP, shot.flip, mode, true).len(),
            });
        }
    }
    out
}

/// 一发几何是否"完全对得上"（三条都逐位相同）。
pub fn geom_exact(g: &ShotGeom) -> bool {
    f64_eq_bits(g.stop_x, g.end_x) && f64_eq_bits(g.stop_x, g.expl_x) && f64_eq_bits(g.stop_y, g.expl_y)
}

// ===========================================================================
// 逐批归因
// ===========================================================================

/// 一批的分歧成因（**互斥**，按"最先出现的那一类"归）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cause {
    /// 全对
    Clean,
    /// 结算批里 ≥2 发（同时回合制按类型分组发 N 发）—— 已知未实现的层
    MultiShot,
    /// `FunctionInfo.corrected_function` 文本
    Corrected,
    /// `FunctionPoints.end_x`
    EndX,
    /// `EffectInfo(Explosion).pos_x`
    PosX,
    /// `EffectInfo(Explosion).pos_y`
    PosY,
    /// 死亡特效多/少/错
    DeathEffect,
    /// 挖洞点
    Destroy,
    /// 其它
    Other,
}

pub fn cause_name(c: &Cause) -> &'static str {
    match c {
        Cause::Clean => "完全一致",
        Cause::MultiShot => "多发批（按类型分组，未实现）",
        Cause::Corrected => "corrected_function 文本",
        Cause::EndX => "FunctionPoints.end_x",
        Cause::PosX => "爆炸点 pos_x",
        Cause::PosY => "爆炸点 pos_y",
        Cause::DeathEffect => "死亡特效",
        Cause::Destroy => "挖洞点",
        Cause::Other => "其它",
    }
}

fn classify(v: &BatchVerdict, multi: bool) -> Cause {
    if v.content_ok && v.order_ok {
        return Cause::Clean;
    }
    if multi {
        return Cause::MultiShot;
    }
    // 单发批：按字段路径归因
    for d in &v.diffs {
        if d.path.contains("corrected_function") {
            return Cause::Corrected;
        }
        if d.path.contains("end_x") {
            return Cause::EndX;
        }
        if d.path.contains("pos_x") {
            return Cause::PosX;
        }
        if d.path.contains("pos_y") {
            return Cause::PosY;
        }
        if d.path.contains("DeathEffect") || d.path.contains(".effect_type") {
            return Cause::DeathEffect;
        }
        if d.path.contains("DestroyObstacle") {
            return Cause::Destroy;
        }
    }
    // 种类序列差：看差在"多/少一个 DeathEffect 块"还是别处。
    for d in &v.diffs {
        if d.path.contains("块的种类") {
            let cnt = |t: &str| -> usize {
                ["DeathEffect", "AshEffect", "SparkEffect"].iter().map(|k| t.matches(k).count()).sum::<usize>()
                    - ["ExplosionEffect"].iter().map(|k| t.matches(k).count()).sum::<usize>()
            };
            if cnt(&d.ours) != cnt(&d.observed) {
                return Cause::DeathEffect;
            }
            return Cause::Other;
        }
    }
    if !v.missing.is_empty() || !v.extra.is_empty() {
        return Cause::DeathEffect;
    }
    Cause::Other
}

/// 一份回放的对拍结论（收敛层的版本）。
#[derive(Debug, Clone, Default)]
pub struct FileResult {
    pub name: String,
    pub scored_batches: usize,
    /// 逐批成因
    pub causes: Vec<Cause>,
    /// 每一批的第一处分歧（`ours` / `observed` 原文）——排障用
    pub first_diffs: Vec<Option<crate::replaydiff::FieldDiff>>,
    pub clean: bool,
    /// 全部字段差异计数（`kind|path|class`）
    pub diff_counts: std::collections::BTreeMap<String, usize>,
    pub geom: Vec<ShotGeom>,
}

/// 收敛层用的口径：**在 `replaydiff::Policy` 之上把 `AshEffect` / `SparkEffect`
/// 也算成"我方产出"**。
///
/// 方向要看清：这是**收紧**不是放宽。`docs/136` 的默认口径把这两类特效当"我方没有
/// 发射器"整类排除；本轮查明它们与 `DeathEffect` 是**同一个发射器按皮肤选类型**
/// （[`death_effect_type`]），所以我们能产出它们 —— 那就必须被比。
pub fn strict_policy() -> Policy {
    Policy {
        produced_effects: &["ExplosionEffect", "DeathEffect", "AshEffect", "SparkEffect"],
        ..Policy::default()
    }
}

/// 跑一份回放：模式感知预测 + 与回放逐批比 + 归因。
pub fn run_file(path: &std::path::Path, policy: &Policy, max_diffs: usize) -> FileResult {
    let mut res = FileResult {
        name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ..Default::default()
    };
    let Ok(replay) = Replay::load(path) else {
        return res;
    };
    let batches = split_batches(&replay);
    if batches.is_empty() {
        return res;
    }
    let inputs = Inputs::scan(&replay, batches[0].start);
    let soldiers: Vec<Target> = inputs
        .pos
        .iter()
        .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
        .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();
    let mut pred = ConvPredictor::new(&inputs.obstacles, soldiers, mode_of(&inputs.room.function_mode));
    // 从**文件开头**扫，第一批之前就有 `SkinGraphicsInfo`（加入链）
    let mut cursor = 0usize;
    for b in batches.iter() {
        for m in &replay.msgs[cursor..b.start] {
            match m.kind.as_str() {
                "FunctionModeInfo" => {
                    if let Some(s) = m.get_str("function_mode") {
                        pred.mode = mode_of(&s);
                    }
                }
                // 皮肤是**输入**（加入链里的 `SkinGraphicsInfo`），不是预测目标
                "SkinGraphicsInfo" => {
                    if let (Some(id), Some(s)) = (m.get_i64("entity_id"), m.get_str("graphics_str")) {
                        pred.skin.insert(id, s);
                    }
                }
                _ => {}
            }
        }
        cursor = b.start;
        if b.shots.is_empty() {
            continue;
        }
        res.scored_batches += 1;
        let tick = replay.msgs[b.start].tick;
        let mut ours: Vec<Msg> = Vec::new();
        for (shot_i, shot) in b.shots.iter().enumerate() {
            let _ = shot_i;
            let (wire, _o) = pred.fire(shot.owner, &shot.text, shot.x0, shot.y0, shot.flip, tick);
            for w in &wire {
                if let Ok(m) = Msg::parse_wire(w) {
                    ours.push(m);
                }
            }
        }
        let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();
        let v = compare_batch(&ours, &observed, policy, max_diffs);
        for d in &v.diffs {
            *res.diff_counts
                .entry(format!("{}|{}|{}", d.kind, d.path, crate::replaydiff::class_name(&d.class)))
                .or_insert(0) += 1;
        }
        res.causes.push(classify(&v, b.shots.len() > 1));
        res.first_diffs.push(v.first.clone());
    }
    res.clean = res.scored_batches > 0 && res.causes.iter().all(|c| *c == Cause::Clean);
    res
}

/// 跑整个语料。
pub fn run_dir(dir: &std::path::Path, policy: &Policy, max_diffs: usize) -> Vec<FileResult> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    names.iter().map(|p| run_file(p, policy, max_diffs)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_follows_function_mode_string() {
        assert_eq!(mode_of("DiffEqFunction"), Mode::Ode);
        assert_eq!(mode_of("NormalFunction"), Mode::Normal);
        // 未知值按 Normal（不猜）
        assert_eq!(mode_of("whatever"), Mode::Normal);
    }

    /// DiffEq 局的修正文本 = 函数原文（回放 550/550）。
    #[test]
    fn diffeq_corrected_text_is_the_raw_function() {
        let f = "-222(-y+(-5+(13)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(20))))))))";
        assert_eq!(corrected_function(f, 3.0, 4.0, false, Mode::Ode), f);
        // 空函数也一样（不加 " + 0.000"）
        assert_eq!(corrected_function("", 3.0, 4.0, false, Mode::Ode), "");
    }

    /// Normal 局 = 函数原文 + 后缀，且后缀带符号（`docs/127`）。
    #[test]
    fn normal_corrected_text_appends_signed_suffix() {
        assert_eq!(
            corrected_function("0.00", 1.651220991201045, 4.638020825690482, false, Mode::Normal),
            "0.00 + 4.638"
        );
        assert_eq!(
            corrected_function("", -13.429861913415694, -3.686102115928783, false, Mode::Normal),
            " - 3.686"
        );
    }

    /// 括号没闭合的输入：真值补右括号再拼后缀（回放 11 条）。
    #[test]
    fn unbalanced_function_gets_closing_brackets() {
        // 回放 1782642986 的原文与真值
        let got = corrected_function("5/155sin(4exp(4*6cos(", 0.0, 6.055, false, Mode::Normal);
        assert!(got.starts_with("5/155sin(4exp(4*6cos()))"), "got = {got}");
    }
}
