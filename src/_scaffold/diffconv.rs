//! 回放消息级对拍的**收敛**层（`docs/142-diff-converge.md`）。
//!
//! `docs/136` 建的 `replaydiff` 是**口径 + 测量**：它把回放切成结算批、用
//! `game::Match::fire` 产报文、逐字节比。它量出了旧基线（完全一致 0/400），
//! 但 `fire` 有两条写死的假设让对拍**永远**过不去：
//!
//! | 写死的地方 | 真值 | 后果 |
//! |---|---|---|
//! | `fire` 恒用 `graphwar_path_calculator::Mode::Normal` | `FunctionModeInfo = DiffEqFunction` 的批必须走 `Mode::Ode` | 550 发的几何整条错 |
//! | `graphwar_shot_calculator::corrected_text` 恒加后缀 | **Normal** 加 `" ± c.ddd"`，**DiffEq 不加**（`corrected_function == function`，550/550 零例外） | 550 发全错 |
//!
//! 本模块**不改内核**（`game.rs` 一字未动）：它自己按
//! `fire` 的顺序拼报文，只是把上面两条换成回放给的真值。**报文顺序、字段名、
//! 浮点写法全部复用 `protocol::emit_*`**，所以"逐字节"这条口径没有变松。
//!
//! **坐标计算不在本模块**（第三轮工程债，`docs/06-verification.md` §30）：
//! 修正文本两串 / 点序列 / 停止点 / 致死 / 爆炸点全部走内核的那**一个**入口
//! [`crate::graphwar_shot_calculator::settle::resolve_shot`]。之前这里自己算过一遍、
//! 与 `game.rs` 那一份并存，已经导致过"同一批数据两条路径结论不同"（`docs/142` §5）。
//! 合并时还查出**第二处**反向分歧：挖洞点这里用采样停点、内核用回代爆炸点（§30.4），
//! 现在也走内核（回放 503/503 条 `DestroyObstacle` ≡ 同批爆炸点）。
//! 现在本模块只剩三样**驱动**职责：采样模式、皮肤表、报文装配。
//!
//! 一致性由 `tests/diffconv_kernel_consistency.rs` 逐位钉住（点序列 / stop / killed /
//! effect 序列；401 份快照上 565 批 740 发 6119 条报文全同，唯一已知差异是
//! `EffectInfo(ExplosionEffect).pos_y`，见 §30.5）。
//!
//! 为什么另起一份而不是给 `Match::fire` 加参数：`fire` 的报文序列已经被
//! `tests/replay_diff.rs` 与端到端测试钉住，而同一时刻有别人在改 `game.rs` 的
//! 同时回合制事务化（`docs/136` §6 第 3 条）。把它做成**新增的、可丢弃的实验层**，
//! 结论成立之后再合并回 `fire`，主线不会被实验污染。
//!
//! 证据档位与仓库约定一致：结论标 **观测**（能指到回放里那一条报文）/
//! **逐位验证**（`tests/diff_conv.rs` 覆盖）/ **推断** / **未知**。

use crate::graphwar_path_calculator::Mode;
use crate::plane::empty_mask;
use crate::_scaffold::replaydiff::{
    compare_batch, split_batches, BatchVerdict, Inputs, Msg, Policy, Replay, ShotInput, StateMask,
};
use crate::graphwar_shot_calculator::settle::{resolve_shot, ShotGeometry, Target};

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
/// 规则本体在**内核**：[`crate::graphwar_shot_calculator::settle::corrected_text`]；
/// 本函数只是"取发出的那一串"的历史投影（`tests/diff_conv.rs` 用它）。
///
/// * `Mode::Ode`（DiffEq 局）：`corrected_function == function`，**不加后缀**
///   —— 全语料 550/550 零例外（回放实读）。
/// * `Mode::Normal`：`to_text(parse(function)) + " ± c.ddd"`。736/740 是
///   `function + suffix` 的字面拼接，剩下 4 条是空函数（`" + 3.333"`）；
///   另有 11 条函数**括号没闭合**，真值补了右括号（例：
///   `5/155sin(4exp(4*6cos(` → `5/155sin(4exp(4*6cos())) + 6.055`）。
pub fn corrected_function(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode) -> String {
    crate::graphwar_shot_calculator::settle::corrected_text(text, x0, y0, flip, mode).emitted
}

/// 字符串里没闭合的 `(` 个数（`(` 减 `)`, 下限 0）。
///
/// 规则本体在**内核**（同一份实现）。为什么不能直接用 `math_function::to_text`：
/// 真游戏的 `to_text` **原样保留**输入文本（740 条 Normal 里 736 条是
/// `function + 后缀` 的字面拼接），唯一的差别是**没闭合的括号会被补齐**（11 条，
/// 例 `5/155sin(4exp(4*6cos(` → `5/155sin(4exp(4*6cos()))`），而本项目的
/// `math_function::to_text` 会重新加括号
/// （`to_text(parse("(40)*(1/(...))")) != "(40)*(1/(...))"`）。
pub fn unbalanced_brackets(text: &str) -> usize {
    crate::graphwar_shot_calculator::settle::unbalanced_brackets(text)
}

/// 爆炸点：**普通模式下它不是采样器的停点**。
///
/// 观测（739 发 Normal，`docs/142` §4.2）：把回放的 `FunctionPoints.end_x`
/// 代回**回放自己那条 `corrected_function` 文本**求值，得到的 y 与回放的
/// `EffectInfo(Explosion).pos_y` **逐位相同 529/739**、1~2 ulp 内 92 发；
/// 而我们采样器的停点 y 只有 **33/739**。⇒ 真游戏的爆炸点是"拿印刷版修正函数
/// 文本在 `end_x` 处求一遍"，不是采样器的最后一个采样点。
///
/// 规则本体在**内核**：[`crate::graphwar_shot_calculator::settle::explosion_point`]
/// （`game::hole_point` 是同一条规则的另一份实现，两边由 `settle` 的单元测试钉住；
/// 空函数那一例是**已知分歧**，见该测试）。
pub fn explosion_point(corrected: &str, stop: (f64, f64), flip: bool, mode: Mode) -> (f64, f64) {
    crate::graphwar_shot_calculator::settle::explosion_point(corrected, stop, flip, mode)
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
    ///
    /// **它就是 `N = 1` 的 [`ConvPredictor::fire_batch`]**：报文逐字节相同
    /// （`mod tests::single_shot_is_byte_identical_to_fire_batch` 钉住）。
    pub fn fire(
        &mut self,
        owner_id: i64,
        function_text: &str,
        x0: f64,
        y0: f64,
        flip: bool,
        now_tick: i64,
    ) -> (Vec<String>, ShotOutcome) {
        let shot = ShotInput {
            owner: owner_id,
            x0,
            y0,
            flip,
            text: function_text.to_string(),
        };
        let (msgs, mut outs) = self.fire_batch(std::slice::from_ref(&shot), now_tick);
        (msgs, outs.remove(0))
    }

    /// **一次结算批**的几何：逐发调内核的那**一个**入口（`settle::resolve_shot`）。
    ///
    /// 纯函数（不改状态），所以"报文里的坐标来自哪里"可以单独测：
    /// `tests/diffconv_kernel_consistency.rs` 拿它的 `points/stop/killed/explosion`
    /// 与 `game::Match::fire_functions` 的产**逐位**比。
    ///
    /// 射手本人的排除**不在本层**：内核只比 id（`docs/122` §4），
    /// 所以这里把整张士兵表原样递下去、`owner` 也一起递。
    pub fn plan(&self, shots: &[ShotInput]) -> Vec<ShotGeometry> {
        shots
            .iter()
            .map(|s| {
                resolve_shot(
                    &s.text,
                    s.x0,
                    s.y0,
                    crate::SERVER_STEP,
                    s.flip,
                    self.mode,
                    &self.mask,
                    &self.soldiers,
                    Some(s.owner),
                )
            })
            .collect()
    }

    /// **一次结算批**：同时回合制的一个事务（`docs/141` §1–§3），报文顺序
    ///
    /// ```text
    /// GameStateInfo(CalculatingResults)
    /// FunctionInfo(owner_i, 原文, 修正后) × N     ← 按类型分组，顺序 = 批内发序
    /// FunctionPoints(owner_i, 起点, end_x) × N    ← 同一顺序
    /// GameStateInfo(SynchronizingResults)
    /// [EffectInfo(死亡/灰烬/火花)] × D            ← 按弹丸**飞行时间**（弧长）升序
    /// [QueueDestroyObstacle] × B                 ← 回放不记录，位置无证据
    /// (FunctionActive(owner_i) + EffectInfo(Explosion)) × N   ← 每发一对，同一顺序
    /// GameStateInfo(ExecutingResults)
    /// ```
    ///
    /// 与 [`ConvPredictor::fire`] 的差别只有两条，都是 `docs/141` 里 130 个真多发包
    /// 130/130 对上的规则：
    ///
    /// 1. **报文按类型分组**（逐发各开一个窗口的话，`GameStateInfo` 会是 3N 条而不是 3 条；
    ///    归因表里 `S` 123 + `SD` 7 = 130 = 全部多发批，症状都是这一条）；
    /// 2. **几何并行**：每一发都对着**批开始那一刻**的士兵表算（`docs/141` §2.1，
    ///    死亡集合恰好等于"逐发独立"的并集，152/152）；谁死不改变别人的弹道。
    ///
    /// 死亡块里同一名士兵**只出现一次**，归属**最早到达**的那一发：同一回合所有弹丸的
    /// `active_tick` 相同（13/13 回合），所以"谁先到"⇔"谁的弧长更短"，
    /// 排序键取 `(弧长, 发序号, 士兵 id)`；全体并列的兜底纯为确定性，
    /// 真游戏在"两发同时到达"时怎么判仍是 **未知**（`docs/141` §5 第 5 条）。
    ///
    /// `N = 1` 时本函数与 [`ConvPredictor::fire`] 逐字节相同 ⇒ 单发批**不可能**变差。
    pub fn fire_batch(
        &mut self,
        shots: &[ShotInput],
        now_tick: i64,
    ) -> (Vec<String>, Vec<ShotOutcome>) {
        // ---- 结算阶段 ----
        // ① 几何并行：先按批开始那一刻的士兵表把每一发算完，再动状态。
        //    坐标**全部**来自内核（[`ConvPredictor::plan`]），本层不做任何算式。
        let geoms = self.plan(shots);
        let resolved: Vec<_> = geoms.iter().map(|g| g.outcome.clone()).collect();
        let corrected: Vec<String> = geoms.iter().map(|g| g.corrected.emitted.clone()).collect();

        // ② 合并死亡：按 (弧长, 发序号, 士兵 id) 升序，同一士兵只留最早到达的那一发。
        let mut merged: Vec<(f64, usize, i64)> = Vec::new();
        for (si, out) in resolved.iter().enumerate() {
            for h in &out.hits {
                merged.push((h.arc, si, h.id));
            }
        }
        merged.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
                .then(a.2.cmp(&b.2))
        });
        let mut deaths: Vec<i64> = Vec::with_capacity(merged.len());
        for (_, _, id) in &merged {
            if !deaths.contains(id) {
                deaths.push(*id);
            }
        }

        // ---- 广播阶段 ----
        let mut msgs = vec![crate::_scaffold::protocol::emit_game_state("CalculatingResults")];
        // ③ 前缀按类型分组（`docs/141` §3.1 ①②）
        for (s, c) in shots.iter().zip(corrected.iter()) {
            msgs.push(crate::_scaffold::protocol::emit_function_info(
                s.owner,
                &s.text,
                Some(c),
            ));
        }
        for (s, out) in shots.iter().zip(resolved.iter()) {
            msgs.push(crate::_scaffold::protocol::emit_function_points(
                s.owner, s.x0, s.y0, out.stop.0,
            ));
        }
        msgs.push(crate::_scaffold::protocol::emit_game_state("SynchronizingResults"));

        // ④ 死亡块：顺序 = 到达时刻；特效类型照抄**出生登记**（[`death_effect_type`]）
        for id in &deaths {
            msgs.push(crate::_scaffold::protocol::emit_life_queued_death(*id, now_tick));
            let Some(t) = self.soldiers.iter().find(|t| t.id == *id) else {
                continue;
            };
            self.next_effect_id += 1;
            let et = death_effect_type(self.skin.get(id).map(|s| s.as_str()));
            msgs.push(crate::_scaffold::protocol::emit_effect_info(
                self.next_effect_id,
                t.x,
                t.y,
                et,
                now_tick,
            ));
        }

        // ⑤ 挖洞块（回放不记录 `QueueDestroyObstacle`，位置**无证据**；沿用单发实现的
        //    "死亡之后、`FunctionActive` 之前"）
        //
        //    位置的**证据**在 `DestroyObstacle` 那侧：挖洞点 = **爆炸点**，不是采样停点
        //    （`docs/14-turn.md` §12：全语料 503/503 条 `DestroyObstacle` 与同批
        //    `EffectInfo(ExplosionEffect)` 逐位相同）。这条规则在内核里
        //    （`game::hole_point` / [`settle::explosion_point`]），本层**不再自己用停点**
        //    —— 旧写法这里发的是 `out.stop`，与内核的 ③ 互为镜像，属于本轮合并掉的
        //    第二处坐标分歧（`docs/06-verification.md` §30.4）。
        for (_, g) in shots.iter().zip(geoms.iter()) {
            if !g.outcome.blocked {
                continue;
            }
            let (hx, hy) = g.explosion;
            msgs.push(format!(
                r#"{{"QueueDestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5,"tick":{}}}}}"#,
                hx, hy, now_tick
            ));
            crate::plane::rasterize(&mut self.mask, hx, hy, 0.5, 0xFF);
            self.pending_destroy.push((
                now_tick + DESTROY_DELAY_TICKS,
                format!(
                    r#"{{"DestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5}}}}"#,
                    hx, hy
                ),
            ));
        }

        // ⑥ 每发一对 `FunctionActive` + `EffectInfo(Explosion)`，顺序与前缀相同
        for (s, g) in shots.iter().zip(geoms.iter()) {
            msgs.push(crate::_scaffold::protocol::emit_function_active(s.owner, now_tick));
            self.next_effect_id += 1;
            // 爆炸点来自内核（`settle::explosion_point`，`Mode::Ode` 时就是停点）
            let (ex, ey) = g.explosion;
            msgs.push(crate::_scaffold::protocol::emit_effect_info(
                self.next_effect_id,
                ex,
                ey,
                "ExplosionEffect",
                now_tick,
            ));
        }
        msgs.push(crate::_scaffold::protocol::emit_game_state("ExecutingResults"));

        // ---- 状态落地（报文里的坐标/类型都来自批开始时的快照）----
        if !deaths.is_empty() {
            self.soldiers.retain(|t| !deaths.contains(&t.id));
        }

        let outs = shots
            .iter()
            .zip(geoms.into_iter())
            .map(|(s, g)| ShotOutcome {
                owner: s.owner,
                function: s.text.clone(),
                corrected: g.corrected.emitted,
                stop: g.outcome.stop,
                blocked: g.outcome.blocked,
                killed: g.outcome.killed,
            })
            .collect();
        (msgs, outs)
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
                        if matches!(m.get("alive").map(|v| v.kind.clone()), Some(crate::_scaffold::replaydiff::wire::Kind::Bool(false))) {
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
                    && matches!(m.get("alive").map(|x| x.kind.clone()), Some(crate::_scaffold::replaydiff::wire::Kind::Bool(false)))
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
            // 我方几何：**一次**内核调用（点序列 / stop / killed / 爆炸点都在里面）。
            // 旧写法在这里把同一条弹道采样了 **5 次**（`resolve` 一次、`pts` 一次、
            // `h2`/`h2_n`/`our_n` 各一次），这正是"两处各算一遍"的温床。
            let targets: Vec<Target> = alive
                .iter()
                .filter_map(|id| pos.get(id).map(|(x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 }))
                .collect();
            let g = resolve_shot(
                &shot.text,
                shot.x0,
                shot.y0,
                crate::SERVER_STEP,
                shot.flip,
                mode,
                mask.mask(),
                &targets,
                Some(shot.owner),
            );
            let tail: Vec<(f64, f64)> = g
                .outcome
                .points
                .iter()
                .rev()
                .take(4)
                .rev()
                .copied()
                .collect();
            let n_points = g.outcome.points.len();
            // H1/H2 是对**回放自己那条 `corrected_function`** 的假设检验（输入来自回放，
            // 不是我们的坐标）：H1 = 回代求值，H2 = 整条重采样。各算一次，共用结果。
            let obs_t = obs_corrected.get(shot_i).cloned().unwrap_or_default();
            let obs_xe = obs_end_x.get(&shot.owner).copied().unwrap_or(f64::NAN);
            let h2_pts = crate::graphwar_shot_calculator::shot_points(
                &obs_t,
                shot.x0,
                shot.y0,
                crate::SERVER_STEP,
                shot.flip,
                mode,
                true,
            );
            out.push(ShotGeom {
                file: name.clone(),
                batch: idx,
                shots_in_batch: b.shots.len(),
                owner: shot.owner,
                mode: if mode == Mode::Ode { "DiffEq" } else { "Normal" },
                turn_mode: turn_mode.clone(),
                function: shot.text.clone(),
                stop_x: g.outcome.stop.0,
                stop_y: g.outcome.stop.1,
                blocked: g.outcome.blocked,
                end_x: obs_xe,
                expl_x: obs_expl.get(&shot.owner).map(|v| v.0).unwrap_or(f64::NAN),
                expl_y: obs_expl.get(&shot.owner).map(|v| v.1).unwrap_or(f64::NAN),
                n_points,
                tail,
                killed: g.outcome.killed.clone(),
                obs_death_effects,
                obs_died: obs_died.clone(),
                corrected_ours: g.corrected.emitted.clone(),
                corrected_obs: obs_t.clone(),
                flip: shot.flip,
                h1_y: {
                    if obs_t.is_empty() {
                        f64::NAN
                    } else {
                        let toks = crate::math_function::parse(&obs_t);
                        crate::math_function::eval(&toks, if shot.flip { -obs_xe } else { obs_xe }, 0.0, 0.0)
                    }
                },
                h2: {
                    let last = h2_pts.last().copied().unwrap_or((f64::NAN, f64::NAN));
                    (last.0, last.1)
                },
                h2_n: h2_pts.len(),
                our_n: n_points,
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
    /// **多发批的老症状**：逐发各开一个结算窗口 ⇒ `GameStateInfo` 是 3N 条而不是 3 条。
    ///
    /// 2026-11-07 第二轮接线（`docs/06` §25）之前，这一档把**所有** ≥2 发的批都吞了；
    /// 现在只在**种类计数确实差 `GameStateInfo`** 时才算（与
    /// `tools/bit_exact_attrib.py` 的 `S` 桶同一条判据），否则落到下面的字段级归因 ——
    /// 否则"结构已对、值还错"的多发批会被误报成结构问题。
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
        Cause::MultiShot => "多发批结构（逐发窗口未分组，GameStateInfo 3N 条）",
        Cause::Corrected => "corrected_function 文本",
        Cause::EndX => "FunctionPoints.end_x",
        Cause::PosX => "爆炸点 pos_x",
        Cause::PosY => "爆炸点 pos_y",
        Cause::DeathEffect => "死亡特效",
        Cause::Destroy => "挖洞点",
        Cause::Other => "其它",
    }
}

/// 种类序列差里 `GameStateInfo` 的条数差（我方 − 回放）。
///
/// `> 0` ⇒ 就是"逐发各开一个窗口"那个老症状（`docs/141` §1 的 `S` 桶）。
fn state_gap(v: &BatchVerdict) -> i64 {
    for d in &v.diffs {
        if d.path.contains("块的种类") {
            let c = |t: &str| t.matches("GameStateInfo").count() as i64;
            return c(&d.ours) - c(&d.observed);
        }
    }
    0
}

fn classify(v: &BatchVerdict, multi: bool) -> Cause {
    if v.content_ok && v.order_ok {
        return Cause::Clean;
    }
    // 只有"种类计数确实差 GameStateInfo"的多发批才算结构问题（第二轮接线后应为 0）
    if multi && state_gap(v) > 0 {
        return Cause::MultiShot;
    }
    // 结构已对的批（含接线后的多发批）：按字段路径归因
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
    pub first_diffs: Vec<Option<crate::_scaffold::replaydiff::FieldDiff>>,
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
        // 一个结算批 = 一个事务（`docs/141` §3）：**一次** `fire_batch` 产出整批。
        // 旧写法是逐发 `fire` 再把报文拼起来 —— 那是归因表里 `S` 桶 123 批的全部内容。
        let (wire, _outs) = pred.fire_batch(&b.shots, tick);
        let mut ours: Vec<Msg> = Vec::new();
        for w in &wire {
            if let Ok(m) = Msg::parse_wire(w) {
                ours.push(m);
            }
        }
        let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();
        let v = compare_batch(&ours, &observed, policy, max_diffs);
        for d in &v.diffs {
            *res.diff_counts
                .entry(format!("{}|{}|{}", d.kind, d.path, crate::_scaffold::replaydiff::class_name(&d.class)))
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

    /// 非有限值不是坐标：`±inf` 会写成非法 JSON（`"pos_y":-inf`），回退到停点。
    #[test]
    fn non_finite_explosion_point_falls_back_to_stop() {
        let stop = (2.0, 3.5);
        // `1/(0)` 在 x=0 处求值是 +inf
        assert_eq!(explosion_point("1/(0)", stop, false, Mode::Normal), stop);
        assert_eq!(explosion_point("-1/(0)", stop, false, Mode::Normal), stop);
        // 负样本：有限值照常回代，不回退
        let (x, y) = explosion_point("0.00", stop, false, Mode::Normal);
        assert_eq!(x, stop.0);
        assert_eq!(y, 0.0);
    }

    // -----------------------------------------------------------------------
    // 多发批结构（`docs/141` §3）：分组规则 + "N = 1 逐字节不变"
    // -----------------------------------------------------------------------

    /// 两个士兵：1 号在原点、7 号在 **(0, 10)** —— 两条 `y = 常数` 的弹道都不穿过对方
    /// （距离 10 > 半径 0.5），所以这批**一发都打不死**，可以干净地钉报文结构。
    fn two_soldiers() -> ConvPredictor {
        ConvPredictor::new(
            &[],
            vec![
                Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 },
                Target { id: 7, x: 0.0, y: 10.0, radius: 0.5 },
            ],
            Mode::Normal,
        )
    }

    fn kinds(wire: &[String]) -> Vec<String> {
        wire.iter()
            .map(|w| {
                let m = Msg::parse_wire(w).expect("我方 emitter 产出了非法 JSON");
                if m.kind == "EffectInfo" {
                    format!("EffectInfo/{}", m.get_str("effect_type").unwrap_or_default())
                } else {
                    m.kind.clone()
                }
            })
            .collect()
    }

    /// **单发批逐字节不变**：`fire` ≡ `fire_batch(N=1)`。
    ///
    /// 这是"接线不许让单发批变差"的**机械化保证**（不是"跑一遍看着没变"）：
    /// `fire` 现在就是这个函数的一发特例。
    #[test]
    fn single_shot_is_byte_identical_to_fire_batch() {
        let text = "0.00";
        let (a, oa) = two_soldiers().fire(1, text, 0.0, 0.0, false, 100);
        let shot = ShotInput { owner: 1, x0: 0.0, y0: 0.0, flip: false, text: text.into() };
        let (b, ob) = two_soldiers().fire_batch(&[shot], 100);
        assert_eq!(a, b, "N=1 的报文必须逐字节相同");
        assert_eq!(oa.stop, ob[0].stop);
        assert_eq!(oa.killed, ob[0].killed);
        assert_eq!(oa.corrected, ob[0].corrected);
    }

    /// 两发一批：**前缀按类型分组、后缀每发一对**（`docs/141` §3.1）。
    #[test]
    fn two_shot_batch_is_grouped_by_type() {
        let shots = vec![
            ShotInput { owner: 1, x0: 0.0, y0: 0.0, flip: false, text: "0.00".into() },
            ShotInput { owner: 7, x0: 0.0, y0: 10.0, flip: true, text: "0.00".into() },
        ];
        let (wire, outs) = two_soldiers().fire_batch(&shots, 100);
        assert_eq!(outs.len(), 2);
        let k = kinds(&wire);
        assert_eq!(
            k,
            vec![
                "GameStateInfo",
                "FunctionInfo",
                "FunctionInfo",
                "FunctionPoints",
                "FunctionPoints",
                "GameStateInfo",
                "FunctionActive",
                "EffectInfo/ExplosionEffect",
                "FunctionActive",
                "EffectInfo/ExplosionEffect",
                "GameStateInfo",
            ],
            "= docs/141 §3.1 的分组：前缀按类型连排、后缀每发一对"
        );
        // 负样本：逐发各开一个窗口的老写法有 **6** 条 `GameStateInfo`（3/发）
        let mut old = Vec::new();
        for s in &shots {
            let (w, _) = two_soldiers().fire(s.owner, &s.text, s.x0, s.y0, s.flip, 100);
            old.extend(kinds(&w));
        }
        assert_eq!(old.iter().filter(|x| x.as_str() == "GameStateInfo").count(), 6);
        assert_ne!(old, k);
    }

    /// 死亡块**按飞行时间**排、同一士兵只出现一次（`docs/141` §2.2）。
    #[test]
    fn death_block_follows_flight_time() {
        // 两条弹道都扫过 7 号：发 0 从 1 号出发（先 9 后 7）、发 1 从 9 号出发（先到 7）
        let mut p = ConvPredictor::new(
            &[],
            vec![
                Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 },
                Target { id: 9, x: 1.0, y: 0.0, radius: 0.5 },
                Target { id: 7, x: 15.0, y: 0.0, radius: 0.5 },
            ],
            Mode::Normal,
        );
        let shots = vec![
            ShotInput { owner: 1, x0: 0.0, y0: 0.0, flip: false, text: "0.00".into() },
            ShotInput { owner: 9, x0: 1.0, y0: 0.0, flip: false, text: "0.00".into() },
        ];
        let (wire, outs) = p.fire_batch(&shots, 100);
        // 几何并行：两发都对着批开始的士兵表算，谁死不影响别人
        assert_eq!(outs[0].killed, vec![9, 7]);
        assert_eq!(outs[1].killed, vec![7]);
        let mut death_effects: Vec<(f64, f64)> = Vec::new();
        for w in &wire {
            let m = Msg::parse_wire(w).unwrap();
            if m.kind == "EffectInfo" && m.get_str("effect_type").as_deref() == Some("DeathEffect") {
                death_effects.push((
                    m.get_f64("pos_x").unwrap_or(f64::NAN),
                    m.get_f64("pos_y").unwrap_or(f64::NAN),
                ));
            }
        }
        // 7 号被两发都扫过 ⇒ 只死一次；死者位置来自**批开始**的快照（x = 15，不是 1）
        assert_eq!(death_effects.len(), 2, "9 号与 7 号各一条");
        assert_eq!(death_effects[0].0, 1.0);
        assert_eq!(death_effects[1].0, 15.0);
        // 状态落地：两名死者都从士兵表里移除
        assert_eq!(p.soldiers.len(), 1);
        assert_eq!(p.soldiers[0].id, 1);
    }
}
