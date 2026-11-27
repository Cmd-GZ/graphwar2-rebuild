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
//! 本模块**不改内核**（`game.rs` / `graphwar_shot_calculator/settle.rs` / `_scaffold/protocol.rs` 一字未动）：它自己按
//! `fire` 的顺序拼报文，只是把上面两条换成回放给的真值。**报文顺序、字段名、
//! 浮点写法全部复用 `protocol::emit_*`**，所以"逐字节"这条口径没有变松。
//!
//! ## 坐标一律走内核（2026-11-07，`docs/06` §30）
//!
//! 本模块**曾经**自己算过一遍坐标（修正文本 / 采样串 / 点序列 / 停点 / 致死 / 爆炸点 /
//! 挖洞点 / 目标排除），于是同一条规则有两份实现，`dispatch/reports/CG-round2` §5 点名
//! 的"两条路径给出不同结论"就是这么来的。现在这七处**全部改成调内核同一个函数**：
//!
//! | 规则 | 内核入口（`pub(crate)`，本模块直接调） |
//! |---|---|
//! | 修正文本（补括号 + `± c.ddd`） | [`crate::game::corrected_function_text`] |
//! | 采样串（多删少补括号 + 后缀） | [`crate::game::sampling_function_text`] |
//! | 点序列 | [`crate::graphwar_shot_calculator::shot_points`]（`apply_correction=false`） |
//! | 停点 / 阻断 / 致死 / 命中弧长 | [`resolve_points_owned`]（射手排除在内核里，只比 id） |
//! | 爆炸点 / 挖洞点 | [`crate::game::hole_point`] |
//!
//! 本模块只剩三样**没有内核对应物**的东西，都写明理由：
//! * [`mode_of`]：回放字符串 → 采样模式。`game::fire_functions` 把 `Mode::Normal` **写死**
//!   （`docs/06` §30.4），所以 DiffEq 局在**内核开火路径里根本不存在**，这条映射只能在这；
//! * [`Mode`] 的分派（`Ode` 时采样原文、修正文本 = 原文）：同上，内核没有 Ode 开火路径；
//! * [`explosion_point`]：**旧签名助手**（吃"已印出的文本"），只被测试/探针用；
//!   `tests/diff_conv.rs` 有一条机械等价性断言钉住它与 `game::hole_point` 逐位相同。
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
use crate::graphwar_shot_calculator::settle::{resolve_points_owned, Target};

/// `QueueDestroyObstacle` → `DestroyObstacle` 的延迟（与 `game.rs` 同一个常量）。
pub use crate::game::DESTROY_DELAY_TICKS;

/// 回放里 `FunctionModeInfo` 的字符串 → 采样模式。
///
/// `NormalFunction` ⇒ `y = f(x)`；`DiffEqFunction` ⇒ `dy/dx = f(x, y)`（RK4）。
/// 未知值按 `Normal`（`docs/136` §3 的语料里只有这两个值，`docs/78`）。
///
/// ⚠ 2026-11-08（`docs/06` §33）：**映射已经进内核**（`game::function_mode_of` +
/// `game::sampling_mode`，`Match` 上也有 `function_mode` 字段与
/// `set_function_mode`）。本函数只是保留旧名字给本模块与探针用，实现直接转调内核，
/// 不再是一份独立的规则。
pub fn mode_of(function_mode: &str) -> Mode {
    crate::game::sampling_mode(crate::game::function_mode_of(function_mode))
}

/// 修正文本的**唯一**规则 —— 直接转调内核（`docs/06` §30.2 / §33.2）。
///
/// * `Mode::Normal`：**就是** [`crate::game::corrected_function_text`]。
///   内核那份是 `docs/06` §29 修出来的真值形状：`补齐缺失的 ")" + 保留多余的 ")"`
///   作为**发出去**的串，而修正常数 `c` 由**去掉多余 ")" 的树**算（真解析器不会被多余的
///   `)` 截断）。旧实现自己拼，且 `c` 用的是**原文**解析出来的树 ⇒ 那 8 条多余的 `)`
///   会让后缀与真值不同。
/// * `Mode::Ode`（DiffEq 局）：`corrected_function == function`，**不加后缀**
///   —— 全语料 550/550 零例外（回放实读）。这条规则原先只在本模块里，现在与内核
///   共用同一个实现（`game::corrected_function_text_mode`）。
pub fn corrected_function(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode) -> String {
    crate::game::corrected_function_text_mode(mode, text, x0, y0, flip)
}

/// **采样器吃的串** —— 内核同一条规则（`docs/06` §25 / §30.2 / §33.2）。
///
/// `Normal` 直接转调 [`crate::game::sampling_function_text`]：与**发出去**的修正文本的
/// 唯一差别是多出来的 `)` 被去掉（理由见 `game::balanced_texts`）。
/// 旧实现走 `settle::resolve(原文, …)`，那条路内部按 `shifted_tokens`（**全精度** `c`）
/// 平移 —— 与真值差 1e-6~1e-3，正是归因表 `Y2` 那一桶。
///
/// `Ode` 局采样串就是函数原文（不带修正常数），同样转调内核。
fn sampling_text(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode) -> String {
    crate::game::sampling_function_text_mode(mode, text, x0, y0, flip)
}

/// 爆炸点 / 挖洞点 —— 转调内核 [`crate::game::hole_point`]，只补一个 `Mode` 适配。
///
/// `DiffEq` 局不重算、直接回停点（`hole_point` 的 `diff_eq` 参数就是这件事）。
fn hole_point(text: &str, x0: f64, y0: f64, flip: bool, stop: (f64, f64), mode: Mode) -> (f64, f64) {
    crate::game::hole_point(mode == Mode::Ode, text, x0, y0, flip, stop)
}

/// 字符串里没闭合的 `(` 个数（`(` 减 `)`, 下限 0）。
///
/// 真游戏发的是 `to_text(parse(function)) + 后缀`，而它的 `to_text` **原样保留**
/// 输入文本（740 条 Normal 里 736 条是 `function + 后缀` 的字面拼接），唯一的差别
/// 是**没闭合的括号会被补齐**（11 条，例 `5/155sin(4exp(4*6cos(` →
/// `5/155sin(4exp(4*6cos()))`）。本项目的 `math_function::to_text` 会重新加括号
/// （`to_text(parse("(40)*(1/(...))")) != "(40)*(1/(...))"`），所以不能直接用它。
///
/// ⚠️ **本函数已不在任何产出/坐标路径上**（`docs/06` §30.2）：补括号那条规则现在只有
/// 内核一份（`game::balanced_texts`），本模块通过 [`corrected_function`] /
/// [`sampling_text`] 调它。这里保留纯属**兼容**（`tests/diff_conv.rs` 用它钉住
/// "缺几个括号"这个可读的量、探针用它打印），实现是纯词法计数、不参与任何坐标。
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
/// （`graphwar_path_calculator::calculate` 的 `px = if flip { -mid } else { mid }`），所以回代也要取反。
///
/// DiffEq（`Mode::Ode`）不走这条：那时 y 不是一个纯函数值，回代无意义，
/// 实测停点自己就是逐位相同的（376/550）。
///
/// **非有限值（NaN / ±inf）一律回退到停点**。理由不是"好看"：±inf 写进报文会变成
/// `"pos_y":-inf` —— **不是合法 JSON**，下游（`Msg::parse_wire`）整条丢掉，
/// 于是这一批在种类序列上"少一条 `ExplosionEffect`"。语料里恰好 1 批
/// （`1787655980` 第 1 批，回放的 `pos_y` 是有限的 11.36 而我们的求值给 `-inf`），
/// 见 `docs/06-verification.md` §25.7。回退值本身**不是真值**（真值未知），
/// 只是"别发出一个不是数字的坐标"。
///
/// ⚠️ **产出路径已改调内核 [`crate::game::hole_point`]**（`docs/06` §30.3）：报文里的
/// 爆炸点/挖洞点现在由 `hole_point` 给。本函数保留是因为**入参不同** —— 它吃
/// **已经印出来的**修正文本，而 `hole_point` 吃**函数原文**并在内部印一遍
/// （补括号 + 三位小数后缀）。两者对同一条原文必须逐位一致，这由本模块的单元测试
/// `tests::explosion_point_is_the_kernel_hole_point`（单点，含空函数那条边界）与
/// `examples/diffconv_parity` + `tests/diffconv_parity.rs`（全语料 740 发）机械钉住
/// —— 不然"同一件事两处各算一遍"就又回来了。
pub fn explosion_point(corrected: &str, stop: (f64, f64), flip: bool, mode: Mode) -> (f64, f64) {
    if mode != Mode::Normal {
        return stop;
    }
    let toks = crate::math_function::parse(corrected);
    if toks.is_empty() {
        return stop;
    }
    let x = if flip { -stop.0 } else { stop.0 };
    let y = crate::math_function::eval(&toks, x, 0.0, 0.0);
    if !y.is_finite() {
        return stop;
    }
    (stop.0, y)
}

/// 我方的一发预测（顺便留下几何量给归因/一致性对拍用）。
#[derive(Debug, Clone)]
pub struct ShotOutcome {
    pub owner: i64,
    pub function: String,
    /// 修正文本（我方）= 内核 `game::corrected_function_text` 的产出
    pub corrected: String,
    /// **采样串**（我方）= 内核 `game::sampling_function_text` 的产出（`Ode` 时 = 原文）
    pub sampling: String,
    /// 弹道点序列（含起点）—— 与 `game::fire_functions` 返回的 `points` 逐位相同
    pub points: Vec<(f64, f64)>,
    pub stop: (f64, f64),
    pub blocked: bool,
    pub killed: Vec<i64>,
    /// 与 `killed` 一一对应的命中细节（弧长 + 采样点下标）
    pub hits: Vec<crate::graphwar_shot_calculator::settle::Hit>,
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
    ///    ⚠ **"并行"只对士兵表成立，对掩膜不成立**（`docs/06` §34.4 / §42）：同批更早
    ///    爆炸的那几发会挖洞，后解算的那几发**看得见**那些洞。所以第二遍按到达序逐发
    ///    重解（[`crate::game::settle_in_order_with_holes`]）—— 与内核**同一个函数**。
    ///    第一遍（上面 ①）算出来的停点只用来给第二遍定**序**。
    ///
    /// 死亡块里同一名士兵**只出现一次**，归属**最早到达**的那一发：同一回合所有弹丸的
    /// `active_tick` 相同（13/13 回合），所以"谁先到"⇔"谁的弧长更短"，
    /// 排序键取 `(弧长, 发序号, 士兵 id)`；全体并列的兜底纯为确定性，
    /// 真游戏在"两发同时到达"时怎么判仍是 **未知**（`docs/141` §5 第 5 条）。
    ///
    /// `N = 1` 时本函数与 [`ConvPredictor::fire`] 逐字节相同 ⇒ 单发批**不可能**变差。
    ///
    /// 批内**结算顺序**（`docs/06` §42）走 [`ConvPredictor::fire_batch_with`]，档位由
    /// [`crate::game::settle_order`] 给（`GW2_SETTLE_ORDER`，默认
    /// [`crate::game::DEFAULT_SETTLE_ORDER`] = S3）——
    /// **与内核 `Match::fire_functions` 读的是同一个开关、同一档**。
    pub fn fire_batch(
        &mut self,
        shots: &[ShotInput],
        now_tick: i64,
    ) -> (Vec<String>, Vec<ShotOutcome>) {
        self.fire_batch_with(shots, now_tick, crate::game::settle_order())
    }

    /// [`ConvPredictor::fire_batch`] 的**显式档位**版本（理由与
    /// [`crate::game::Match::fire_functions_with`] 完全一样：`settle_order()` 读环境变量，
    /// 而 `cargo test` 是多线程的 —— 档位从参数进，产品路径从环境变量进）。
    pub fn fire_batch_with(
        &mut self,
        shots: &[ShotInput],
        now_tick: i64,
        strategy: crate::game::SettleOrder,
    ) -> (Vec<String>, Vec<ShotOutcome>) {
        // ---- 结算阶段 ----
        // ① 几何并行：先按批开始那一刻的士兵表把每一发算完，再动状态。
        //
        // **每一步都是内核同一入口**（`docs/06` §30）：
        //   * 采样串   [`sampling_text`] → `game::sampling_function_text`
        //   * 点序列   `graphwar_shot_calculator::shot_points(…, apply_correction=false)`
        //     （与 `game::fire_functions` 同一个调用、同一个 `SERVER_STEP`）
        //   * 停点/致死 [`resolve_points_owned`] —— **射手由内核按 id 排除**，
        //     调用方不再预筛（旧实现多一条 `1e-12` 位置排除，二进制没有这条）
        //
        // 点列单独留着不只为对拍：第二遍（②）要用它算**到达弧长**。
        let mut resolved = Vec::with_capacity(shots.len());
        let mut samplings: Vec<String> = Vec::with_capacity(shots.len());
        let mut points: Vec<Vec<(f64, f64)>> = Vec::with_capacity(shots.len());
        for s in shots {
            let sampling = sampling_text(&s.text, s.x0, s.y0, s.flip, self.mode);
            let pts = crate::graphwar_shot_calculator::shot_points(
                &sampling,
                s.x0,
                s.y0,
                crate::SERVER_STEP,
                s.flip,
                self.mode,
                false,
            );
            samplings.push(sampling);
            resolved.push(resolve_points_owned(
                &self.mask,
                &self.soldiers,
                &pts,
                Some(s.owner),
            ));
            points.push(pts);
        }

        // ② **批内结算顺序 / 逐发挖洞（S3）** —— 与内核**同一个函数**。
        //
        // 这一段就是"两层为什么同构"的答案：不是各写一遍，而是
        // [`crate::game::settle_in_order_with_holes`] 一个实现、两个驱动器
        // （内核 `Match::fire_functions_with` / 本收敛层）。它按到达序（到停点弧长升序）
        // 重解，每解完一发把洞写进**本批的工作掩膜拷贝**，于是后解的发看得见前面的洞。
        //
        // ⚠ `self.mask` **不在这里改**（函数内部拷一份）：跨批状态仍由 ⑥ 在整批结算完
        // 之后一次性落地，与 `Match::execute_results` ③ 同序同值 —— 只有"同批可见性"
        // 这一个变量被打开。
        //
        // 单发批 / S1 / S2 在函数里**直接返回** ⇒ 与 65 号之前的行为逐位相同。
        let subs: Vec<crate::game::Submission<'_>> = shots
            .iter()
            .map(|s| crate::game::Submission {
                owner_id: s.owner,
                function_text: &s.text,
                x0: s.x0,
                y0: s.y0,
                flip: s.flip,
            })
            .collect();
        crate::game::settle_in_order_with_holes(
            strategy,
            self.mode,
            &self.mask,
            &self.soldiers,
            &subs,
            &points,
            &mut resolved,
        );

        let corrected: Vec<String> = shots
            .iter()
            .map(|s| corrected_function(&s.text, s.x0, s.y0, s.flip, self.mode))
            .collect();

        // ③ 合并死亡：按 (弧长, 发序号, 士兵 id) 升序，同一士兵只留最早到达的那一发。
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
        // ④ 前缀按类型分组（`docs/141` §3.1 ①②）
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

        // ⑤ 死亡块：顺序 = 到达时刻；特效类型照抄**出生登记**（[`death_effect_type`]）
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

        // ⑥ 挖洞块（回放不记录 `QueueDestroyObstacle`，位置**无证据**；沿用单发实现的
        //    "死亡之后、`FunctionActive` 之前"）
        //
        //    **位置 = 爆炸点，不是采样停点**，由内核同一函数 [`hole_point`]
        //    （= `game::hole_point`）给出：全语料 503/503 条 `DestroyObstacle` 与同批
        //    `EffectInfo(ExplosionEffect)` 坐标逐位相同（`docs/14` §12）。旧实现这里
        //    写的是 `out.stop`，所以第二轮任务 14 在 `game.rs` 修好之后本模块没跟上。
        for (s, out) in shots.iter().zip(resolved.iter()) {
            if !out.blocked {
                continue;
            }
            let (hx, hy) = hole_point(&s.text, s.x0, s.y0, s.flip, out.stop, self.mode);
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

        // ⑦ 每发一对 `FunctionActive` + `EffectInfo(Explosion)`，顺序与前缀相同。
        //
        //    位置同样取内核的 [`hole_point`]（挖洞点 ≡ 爆炸点，503/503 逐位）：
        //    `game::execute_results` 的 ④ 目前仍写 `out.stop`（`docs/06` §24 的 `Y` 类、
        //    170 批），那是**内核侧**尚未接线的一处，不是本模块的重复计算 ——
        //    `examples/diffconv_parity` 会把这个差单独计数（§30.5）。
        for (s, out) in shots.iter().zip(resolved.iter()) {
            msgs.push(crate::_scaffold::protocol::emit_function_active(s.owner, now_tick));
            self.next_effect_id += 1;
            let (ex, ey) = hole_point(&s.text, s.x0, s.y0, s.flip, out.stop, self.mode);
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
            .zip(resolved.into_iter())
            .zip(corrected.into_iter().zip(samplings.into_iter()))
            .map(|((s, r), (c, samp))| ShotOutcome {
                owner: s.owner,
                function: s.text.clone(),
                corrected: c,
                sampling: samp,
                points: r.points,
                stop: r.stop,
                blocked: r.blocked,
                killed: r.killed,
                hits: r.hits,
            })
            .collect();
        (msgs, outs)
    }

    /// 到期的 `DestroyObstacle`（与 `Match::tick` **同一份实现**）。
    ///
    /// T72 归一（台账 C 档 `diffconv::tick`）：这里原来有一份与本文件外
    /// [`crate::game::Match::tick`] **逐字符相同**的队列扫描拷贝。真值实现留在
    /// `game.rs`（原版 `handle_tick 0x470840` 的那一段、`Match` 的字段），
    /// 本处改调 [`crate::game::drain_due_destroy`]，**不再有第二份扫描规则**。
    pub fn tick(&mut self, now_tick: i64) -> Vec<String> {
        crate::game::drain_due_destroy(&mut self.pending_destroy, now_tick)
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
    /// 我方步长路径的点数。
    ///
    /// ⚠️ 2026-11（`docs/06` §30）：**= [`ShotGeom::n_points`]**。旧实现这是**第三份**
    /// 采样（`shot_points(原文, apply_correction=true)`，全精度常数），与 `n_points`
    /// 各算一遍；那份重复计算已删，探针的"我方"列现在与预测器逐位同一来源。
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
            // 目标表：**全表交给内核**，射手由 `resolve_points_owned` 按 id 排除
            // （旧实现自己在调用方按 id + `1e-12` 位置双过滤，是同一规则的第二份）。
            let targets: Vec<Target> = alive
                .iter()
                .filter_map(|id| pos.get(id).map(|(x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 }))
                .collect();
            // 几何一律走内核同一入口（与 [`ConvPredictor::fire_batch`] 逐行同构）。
            let sampling = sampling_text(&shot.text, shot.x0, shot.y0, shot.flip, mode);
            let pts = crate::graphwar_shot_calculator::shot_points(
                &sampling,
                shot.x0,
                shot.y0,
                crate::SERVER_STEP,
                shot.flip,
                mode,
                false,
            );
            let r = resolve_points_owned(mask.mask(), &targets, &pts, Some(shot.owner));
            let tail: Vec<(f64, f64)> = r.points.iter().rev().take(4).rev().copied().collect();
            let n_points = r.points.len();
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
                        let toks = crate::math_function::parse(&t);
                        crate::math_function::eval(&toks, if shot.flip { -xe } else { xe }, 0.0, 0.0)
                    }
                },
                h2: {
                    let t = obs_corrected.get(shot_i).cloned().unwrap_or_default();
                    let pts = crate::graphwar_shot_calculator::shot_points(&t, shot.x0, shot.y0, crate::SERVER_STEP, shot.flip, mode, true);
                    let last = pts.last().copied().unwrap_or((f64::NAN, f64::NAN));
                    (last.0, last.1)
                },
                h2_n: {
                    let t = obs_corrected.get(shot_i).cloned().unwrap_or_default();
                    crate::graphwar_shot_calculator::shot_points(&t, shot.x0, shot.y0, crate::SERVER_STEP, shot.flip, mode, true).len()
                },
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

// ===========================================================================
// 两条路径的**逐位**一致性对拍（`docs/06` §30.5，任务 16 的核心验收）
// ===========================================================================
//
// 问题：本模块曾经自己算一遍坐标，于是"同一批回放，两条路径给出不同结论"。
// 现在坐标全走内核（§30.2–§30.4），这条对拍就是**机械化证明**：
//
//   路径 K = `game::Match::fire_functions` + `fire_batch`（真实实现，逐位同 `fire`）
//   路径 C = `ConvPredictor::fire_batch`（收敛层）
//
// 比的是**中间结果**，不是报文是否对上回放：点序列（逐坐标 `to_bits`）/ `stop` /
// `blocked` / `killed`（含顺序）/ 命中弧长 / 修正文本；另外单列挖洞点与爆炸点
// （内核 ④ 仍写 `stop`，见 §30.5 的说明）。
//
// **DiffEq 批 2026-11-08 起参与对拍**（`docs/06` §33）：内核开火路径补上了
// `Mode`（`game::Match::fire_functions` 现在读 `self.sampling_mode()`），
// 于是两条路径在 Ode 局也走同一套坐标。改之前这里是 `continue`（不计入分母），
// 那正是 515 批 / 550 发"不可比"的那一段。

/// 一处不一致（`ours` = 收敛层，`kernel` = 内核路径）。
#[derive(Debug, Clone, PartialEq)]
pub struct ParityMismatch {
    pub file: String,
    pub batch: usize,
    /// 批内第几发（0 起）
    pub shot: usize,
    /// `points` / `stop` / `blocked` / `killed` / `hits` / `corrected` / `destroy` / `explosion`
    pub field: &'static str,
    pub ours: String,
    pub kernel: String,
}

/// 一致性对拍的累计结果。
#[derive(Debug, Clone, Default)]
pub struct Parity {
    /// 读到并至少有一个结算批的回放份数
    pub files: usize,
    /// 参与对拍的结算批（**两条轴合起来**：Normal + DiffEq）
    pub batches: usize,
    /// 参与对拍的发数（**两条轴合起来**）
    pub shots: usize,
    /// 五项（点序列 / stop / blocked / killed / 修正文本）**逐位全同**的发数
    pub shots_identical: usize,
    /// 不一致项总数
    pub mismatches_total: usize,
    /// 前若干处不一致的细节
    pub mismatches: Vec<ParityMismatch>,
    /// 这 `batches` / `shots` 里有多少属于 `DiffEqFunction`（**已计入分母**，
    /// 2026-11-08 起；改之前它们被 `continue` 掉，只计数不比）。
    pub diff_eq_batches: usize,
    pub diff_eq_shots: usize,
    /// `diff_eq_shots` 里**逐位全同**的发数（Ode 轴自己的分子，别拿总和减）。
    pub diff_eq_shots_identical: usize,
    /// 挖洞点（两条路径都对 `hole_point`）：应逐位相同
    pub destroy_compared: usize,
    pub destroy_equal: usize,
    /// 爆炸点：内核 ④ 仍用 `stop`，收敛层用 `hole_point` ⇒ 只计数、不算失败
    pub explosion_compared: usize,
    pub explosion_equal: usize,
}

impl Parity {
    /// "逐位全同"的比例（`"a / b"` 形式，带百分比）。
    ///
    /// **两条轴分开印**（`docs/06` §33）：Normal 与 DiffEq 改用同一条内核入口之后
    /// 都属于"必须逐位相同"，但分母不同（740 / 550），混在一起印容易把
    /// "Ode 轴刚开始比"读成"Normal 轴变差了"。
    pub fn summary(&self) -> String {
        let pct_of = |a: usize, b: usize| -> String {
            if b == 0 {
                "-".to_string()
            } else {
                format!("{:.1}%", 100.0 * a as f64 / b as f64)
            }
        };
        let normal_shots = self.normal_shots();
        let normal_identical = self.shots_identical.saturating_sub(self.diff_eq_shots_identical);
        format!(
            "文件 {}；结算批 {}（Normal {} / DiffEq {}）；发 {}（Normal {} / DiffEq {}）；\
             点序列/stop/blocked/killed/修正文本 逐位全同 {} / {}（{}）；\
             分轴 Normal {} / {}（{}）、DiffEq {} / {}（{}）",
            self.files,
            self.batches,
            self.batches.saturating_sub(self.diff_eq_batches),
            self.diff_eq_batches,
            self.shots,
            normal_shots,
            self.diff_eq_shots,
            self.shots_identical,
            self.shots,
            pct_of(self.shots_identical, self.shots),
            normal_identical,
            normal_shots,
            pct_of(normal_identical, normal_shots),
            self.diff_eq_shots_identical,
            self.diff_eq_shots,
            pct_of(self.diff_eq_shots_identical, self.diff_eq_shots),
        )
    }

    /// 是否**没有**任何不一致（现在涵盖两条轴）。
    pub fn ok(&self) -> bool {
        self.mismatches_total == 0 && self.shots_identical == self.shots
    }

    /// `Normal` 轴上的发数（= 分母减 DiffEq 发数）。
    pub fn normal_shots(&self) -> usize {
        self.shots - self.diff_eq_shots
    }
}

fn bits_pair(v: (f64, f64)) -> (u64, u64) {
    (v.0.to_bits(), v.1.to_bits())
}

/// 对拍一份回放（把结果累加进 `p`；`max_samples` 限制明细条数）。
pub fn parity_file(path: &std::path::Path, p: &mut Parity, max_samples: usize) {
    let Ok(replay) = Replay::load(path) else {
        return;
    };
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let batches = split_batches(&replay);
    if batches.is_empty() {
        return;
    }
    let inputs = Inputs::scan(&replay, batches[0].start);
    let soldiers: Vec<Target> = inputs
        .pos
        .iter()
        .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
        .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();
    let base_tick = replay.msgs[batches[0].start].tick;
    // 路径 K：内核 `Match`（与 `examples/replay_attrib --drive batch` 同一套构造）
    let mut kernel = crate::game::Match::new(
        inputs.room.clone(),
        inputs.obstacles.clone(),
        soldiers.clone(),
        (base_tick, 0),
    );
    for id in inputs.targets.iter() {
        kernel.add_target(*id, 1);
    }
    for (id, g) in inputs.skins.iter() {
        let _ = kernel.note_skin_graphics(*id, g);
    }
    // 路径 C：收敛层预测器
    let mut conv = ConvPredictor::new(
        &inputs.obstacles,
        soldiers,
        mode_of(&inputs.room.function_mode),
    );
    p.files += 1;

    let mut cursor = 0usize;
    let mut mode = conv.mode;
    for (idx, b) in batches.iter().enumerate() {
        // 模式 / 皮肤是**输入**，从文件开头扫（与 `run_file` 同一口径）
        for m in &replay.msgs[cursor..b.start] {
            match m.kind.as_str() {
                "FunctionModeInfo" => {
                    if let Some(s) = m.get_str("function_mode") {
                        mode = mode_of(&s);
                        conv.mode = mode;
                        // 内核路径也要喂 —— 否则两条路径的模式不同，DiffEq 局必然不一致。
                        // （这就是改之前那道 `if diff_eq { continue }` 挡住的东西。）
                        kernel.set_function_mode_wire(&s);
                    }
                }
                "SkinGraphicsInfo" => {
                    if let (Some(id), Some(s)) = (m.get_i64("entity_id"), m.get_str("graphics_str")) {
                        conv.skin.insert(id, s);
                    }
                }
                _ => {}
            }
        }
        cursor = b.start;
        if b.shots.is_empty() {
            continue;
        }
        // DiffEq 批**计入分母**（`docs/06` §33）：内核与收敛层都走 Ode，必须逐位相同。
        if mode == Mode::Ode {
            p.diff_eq_batches += 1;
            p.diff_eq_shots += b.shots.len();
        }
        p.batches += 1;
        let tick = replay.msgs[b.start].tick;

        // ---- 路径 K ----
        kernel.tick_base = (tick, 0);
        let subs: Vec<crate::game::Submission> = b
            .shots
            .iter()
            .map(|s| crate::game::Submission {
                owner_id: s.owner,
                function_text: &s.text,
                x0: s.x0,
                y0: s.y0,
                flip: s.flip,
            })
            .collect();
        // `fire_functions` 只算几何、不改状态，`fire_batch` = 它 + `execute_results`
        let ko = kernel.fire_functions(&subs);
        let wire_k = kernel.fire_batch(&subs, 0);

        // ---- 路径 C ----
        let (wire_c, co) = conv.fire_batch(&b.shots, tick);

        let to_msgs = |wire: &[String]| -> Vec<Msg> {
            wire.iter().filter_map(|w| Msg::parse_wire(w).ok()).collect()
        };
        let kmsgs = to_msgs(&wire_k);
        let cmsgs = to_msgs(&wire_c);

        // 内核**发出去**的修正文本（按 `FunctionInfo` 出现顺序 = 发序号）
        let k_corrected: Vec<String> = kmsgs
            .iter()
            .filter(|m| m.kind == "FunctionInfo")
            .map(|m| m.get_str("corrected_function").unwrap_or_default())
            .collect();

        // 逐发比**中间结果**
        for (i, _s) in b.shots.iter().enumerate() {
            p.shots += 1;
            let (ck, cc) = (&ko[i], &co[i]);
            let mut diffs: Vec<(&'static str, String, String)> = Vec::new();
            if ck.points.len() != cc.points.len() {
                diffs.push((
                    "points",
                    format!("{} 点", cc.points.len()),
                    format!("{} 点", ck.points.len()),
                ));
            } else if let Some(j) = cc
                .points
                .iter()
                .zip(ck.points.iter())
                .position(|(a, b)| bits_pair(*a) != bits_pair(*b))
            {
                diffs.push((
                    "points",
                    format!("第 {j} 点 {:?}", cc.points[j]),
                    format!("第 {j} 点 {:?}", ck.points[j]),
                ));
            }
            if bits_pair(ck.stop) != bits_pair(cc.stop) {
                diffs.push(("stop", format!("{:?}", cc.stop), format!("{:?}", ck.stop)));
            }
            if ck.blocked != cc.blocked {
                diffs.push((
                    "blocked",
                    cc.blocked.to_string(),
                    ck.blocked.to_string(),
                ));
            }
            if ck.killed != cc.killed {
                diffs.push((
                    "killed",
                    format!("{:?}", cc.killed),
                    format!("{:?}", ck.killed),
                ));
            }
            if ck.hits.len() != cc.hits.len()
                || ck
                    .hits
                    .iter()
                    .zip(cc.hits.iter())
                    .any(|(a, b)| a.id != b.id || a.index != b.index || a.arc.to_bits() != b.arc.to_bits())
            {
                diffs.push((
                    "hits",
                    format!("{:?}", cc.hits.iter().map(|h| (h.id, h.index, h.arc)).collect::<Vec<_>>()),
                    format!("{:?}", ck.hits.iter().map(|h| (h.id, h.index, h.arc)).collect::<Vec<_>>()),
                ));
            }
            // 修正文本：内核的这一条是它**发出去**的 `FunctionInfo.corrected_function`
            if let Some(kc) = k_corrected.get(i) {
                if *kc != cc.corrected {
                    diffs.push((
                        "corrected",
                        format!("{:?}", cc.corrected),
                        format!("{:?}", kc),
                    ));
                }
            }
            if diffs.is_empty() {
                p.shots_identical += 1;
                if mode == Mode::Ode {
                    p.diff_eq_shots_identical += 1;
                }
            }
            for (field, ours, kern) in diffs {
                p.mismatches_total += 1;
                if p.mismatches.len() < max_samples {
                    p.mismatches.push(ParityMismatch {
                        file: name.clone(),
                        batch: idx,
                        shot: i,
                        field,
                        ours,
                        kernel: kern,
                    });
                }
            }
        }

        // 挖洞点：两条路径都调 `game::hole_point` ⇒ 应逐位相同
        let (dk, dc) = (destroy_points(&kmsgs), destroy_points(&cmsgs));
        let n_destroy = dk.len().max(dc.len());
        let eq_destroy = dk.iter().zip(dc.iter()).filter(|(a, b)| a == b).count();
        p.destroy_compared += n_destroy;
        p.destroy_equal += eq_destroy;
        if eq_destroy != n_destroy {
            p.mismatches_total += 1;
            if p.mismatches.len() < max_samples {
                p.mismatches.push(ParityMismatch {
                    file: name.clone(),
                    batch: idx,
                    shot: 0,
                    field: "destroy",
                    ours: format!("{dc:?}"),
                    kernel: format!("{dk:?}"),
                });
            }
        }

        // 爆炸点：内核 ④ 仍 `stop`，收敛层 `hole_point` ⇒ 只计数
        let (ek, ec) = (explosion_points(&kmsgs), explosion_points(&cmsgs));
        p.explosion_compared += ek.len();
        p.explosion_equal += ek.iter().zip(ec.iter()).filter(|(a, b)| a == b).count();
    }
}

fn destroy_points(msgs: &[Msg]) -> Vec<(u64, u64)> {
    msgs.iter()
        .filter(|m| m.kind == "QueueDestroyObstacle")
        .map(|m| {
            bits_pair((
                m.get_f64("pos_x").unwrap_or(f64::NAN),
                m.get_f64("pos_y").unwrap_or(f64::NAN),
            ))
        })
        .collect()
}

fn explosion_points(msgs: &[Msg]) -> Vec<(u64, u64)> {
    msgs.iter()
        .filter(|m| {
            m.kind == "EffectInfo"
                && m.get_str("effect_type").as_deref() == Some("ExplosionEffect")
        })
        .map(|m| {
            bits_pair((
                m.get_f64("pos_x").unwrap_or(f64::NAN),
                m.get_f64("pos_y").unwrap_or(f64::NAN),
            ))
        })
        .collect()
}

/// 对整个语料跑一遍一致性对拍。
pub fn parity_dir(dir: &std::path::Path, max_samples: usize) -> Parity {
    let mut p = Parity::default();
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    for n in &names {
        parity_file(n, &mut p, max_samples);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===================================================================
    // T59 真值（**冻结语料 403 份快照**的实测值）—— 观测档
    // ===================================================================
    // 复算：`.reference/venv/bin/python tools/gen_test_expectations2.py`
    //        → `fixtures/test_expectation_truth2.tsv` 的 `D/*`、`G02` 段（`--check` 只比对）。
    // 语料指纹：`fixtures/guardrails.json` 的 `corpus`（403 份逐份 sha256）。
    //
    // 这些常量把本节原来的 **纯 parity**（我方收敛层 vs 我方内核）换成"**真报文长什么样**"：
    // 真值来自回放原文，不是我方两条路径互相印证。

    /// 真语料 1089 个结算批里，**299/299** 的死亡特效早于同批所有爆炸特效（反向 0 例）⇒
    /// "先落命中、再落到达"这条顺序不是我方编的（`D01` 段）。
    const REAL_DEATH_BEFORE_EXPLOSION: (usize, usize) = (299, 0);
    /// 真批里 `ExplosionEffect.pos_x == FunctionPoints.end_x`：**1050/1291 逐位相同**，
    /// **1179/1291 在 1e-4 内**（其余是同一个点的两种算法在末位上的差）⇒ "爆炸点 = 停点/洞点"
    /// 有真报文背书（`G09`/`D02` 段）。
    const REAL_EXPLOSION_AT_END: (usize, usize, usize) = (1050, 1179, 1291);
    /// 真语料的 `FunctionModeInfo` 取值域**只有**这两个（`NormalFunction` 550 / `DiffEqFunction` 320）。
    const REAL_MODE_DOMAIN: [(&str, usize); 2] = [("NormalFunction", 550), ("DiffEqFunction", 320)];
    /// Normal 局定稿报文的 `corrected_function`：**728** 条是 `function + " ± c.ddd"`，
    /// **9** 条例外（括号不平衡，先补 `)` 再拼后缀，见 `D07`）。
    const REAL_NORMAL_SUFFIX: (usize, usize) = (728, 9);
    /// 真语料的括号不平衡**定稿**报文：19 条（`D07` 段）。下面 4 条是原文逐字。
    /// 真语料 4 条括号不平衡的**定稿报文原文**（`function` → `corrected_function`，逐字）：
    /// `1782642986` / `1782674896` / `1782674696`（后两条各一条）。总共 19 条（`D07` 段）。
    const REAL_UNBALANCED: [(&str, &str); 4] = [
        ("5/155sin(4exp(4*6cos(", "5/155sin(4exp(4*6cos())) + 6.055"),
        ("sin(abs(cos(7", "sin(abs(cos(7))) + 5.284"),
        (
            "9/(1+exp(-999(x+1)))+sqrt(4.1-(abs(x)-abs(x-2))^2)",
            "9/(1+exp(-999(x+1)))+sqrt(4.1-(abs(x)-abs(x-2))^2)/2))+sqrt(4.1-(abs(x",
        ),
        (
            "3.5/(1+exp(-999(x)))-10/(1+(999(x-2.5))^2)+sqrt(4.",
            "3.5/(1+exp(-999(x)))-10/(1+(999(x-2.5))^2)+sqrt(4.1-(abs(x-10)-abs(x-1",
        ),
    ];
    /// 真语料 950 个**单射手批**里，射手自己的 id 出现在死亡集合的次数是 **0** ⇒
    /// "射手不杀自己"是真机行为（`G13`/`D04` 段）。
    const REAL_SINGLE_SHOOTER_SELF_KILL: (usize, usize) = (950, 0);
    /// DiffEq 局定稿报文：**550/550** 条 `corrected_function == function`（`G02`/`D09` 段）。
    const REAL_DIFFEQ_VERBATIM: (usize, usize) = (550, 550);

    #[test]
    fn mode_follows_function_mode_string() {
        assert_eq!(mode_of("DiffEqFunction"), Mode::Ode);
        assert_eq!(mode_of("NormalFunction"), Mode::Normal);
        // 未知值按 Normal（不猜）
        assert_eq!(mode_of("whatever"), Mode::Normal);
        // T59 真值：真语料里上线出现的就这两个值（550 / 320 次），且都被映射对
        for (name, _count) in REAL_MODE_DOMAIN {
            let want = if name == "DiffEqFunction" { Mode::Ode } else { Mode::Normal };
            assert_eq!(mode_of(name), want, "真语料取值 {name}");
        }
        assert_eq!(REAL_MODE_DOMAIN[0].1 + REAL_MODE_DOMAIN[1].1, 870, "真语料 FunctionModeInfo 次数");
    }

    /// DiffEq 局的修正文本 = 函数原文（回放 550/550 —— T59 已用冻结语料复算，见 `G02`/`D09`）。
    #[test]
    fn diffeq_corrected_text_is_the_raw_function() {
        let f = "-222(-y+(-5+(13)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(20))))))))";
        assert_eq!(corrected_function(f, 3.0, 4.0, false, Mode::Ode), f);
        // 空函数也一样（不加 " + 0.000"）
        assert_eq!(corrected_function("", 3.0, 4.0, false, Mode::Ode), "");
        // T59 真值：真语料 550/550 条 DiffEq 定稿报文 corrected == function（0 条例外）
        assert_eq!(REAL_DIFFEQ_VERBATIM, (550, 550));
    }

    /// Normal 局 = 函数原文 + 后缀，且后缀带符号（`docs/127`）。
    ///
    /// T59 真值（观测）：真语料 737 条 Normal 定稿报文里 **728** 条是
    /// `corrected_function == function + " ± c.ddd"`、**9** 条例外（括号不平衡类，见 `D07`）。
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
        assert_eq!(REAL_NORMAL_SUFFIX, (728, 9), "真语料的正常/例外条数");
        // 正负号两种后缀都由真语料给出：上面两条手算例子的常数（4.638 / −3.686）来自真报文原文
        assert_eq!(
            corrected_function("0.00", 1.651220991201045, 4.638020825690482, false, Mode::Normal),
            "0.00 + 4.638"
        );
        assert!(
            corrected_function("0.00", 1.0, -3.686102115928783, false, Mode::Normal).contains(" - 3.686"),
            "负后缀必须带符号"
        );
    }

    /// 括号没闭合的输入：真值补右括号再拼后缀（回放 19 条 —— T59 用冻结语料逐条复算）。
    #[test]
    fn unbalanced_function_gets_closing_brackets() {
        // 回放 1782642986 的原文与真值
        let got = corrected_function("5/155sin(4exp(4*6cos(", 0.0, 6.055, false, Mode::Normal);
        assert!(got.starts_with("5/155sin(4exp(4*6cos()))"), "got = {got}");
        // T59 真值：真语料 19 条括号不平衡的**定稿**报文，下面 4 条是原文逐字
        // ⚠️ 后两条在真值表里被截断到 70 字符（原文很长）⇒ 只对前两条完整的验"括号平衡"
        for (k, (raw, want)) in REAL_UNBALANCED.iter().enumerate() {
            assert!(want.starts_with(raw), "报文串必须保留原文：want={want:?} raw={raw:?}");
            if k < 2 {
                assert_eq!(
                    want.matches('(').count(),
                    want.matches(')').count(),
                    "定稿报文必须括号平衡：{want:?}"
                );
            }
        }
        // 我方补齐的形状必须与真报文同族：填平括号后接 ` + 常数`。
        // （后缀的**数值**由这一发的 x0/y0 决定，真值表里没有那一发的起点 ⇒ 只比形状。）
        let got = corrected_function("sin(abs(cos(7", 0.0, 5.284, false, Mode::Normal);
        let real = REAL_UNBALANCED[1].1;
        assert!(got.starts_with("sin(abs(cos(7)))"), "真报文 = {real:?}，我方 = {got:?}");
        assert!(real.starts_with("sin(abs(cos(7)))"), "真报文也必须先补平括号：{real:?}");
        assert_eq!(got.matches('(').count(), got.matches(')').count(), "我方补齐后必须括号平衡");
    }

    /// 非有限值不是坐标：`±inf` 会写成非法 JSON（`"pos_y":-inf`），回退到停点。
    ///
    /// T59：**换不了**（如实登记）。403 份语料 1291 条 `FunctionPoints` + 2243 条 `EffectInfo` 里
    /// **没有一次** `pos` 是 NaN/±inf（`D08` 段）⇒ 真机在这条路径上没有样本。这一条保留为
    /// **契约断言**（回退不会写出非法 JSON），不当成复原证据。
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
    // =======================================================================
    // T46：**冻结语料真值**（观测级）
    // =======================================================================
    //
    // 来源：`.reference/replays/gw2-replays-snapshot/replays/`（403 份冻结快照，
    // 逐份 sha256 登记在 `fixtures/guardrails.json` 的 `corpus`）。
    // 下面两个回放的 sha256：
    //   1782642582 = fbed66caf5dbc2610a28776176366da0a22816f67d47c3e218c722f408d598e9
    //   1782640359 = b4615ff7267f9ab657d9bc1907cb2a36ca28622951e4282764c12901898a75d2
    // 起点态复算：`tools/replay_state.py <snapshot>/<文件名> --index 0`
    // （障碍按 `ClearObstacles`/`AddObstacle` 顺序攒、士兵取 `PosInfo` 且 `LifeInfo` 未标死）。
    // 报文逐条抄自回放原文（只去掉外层 `{"event": …}` 包装）。

    /// `1782642582` 第 0 批之前的 28 个障碍（关卡几何，真值）。
    const REAL_OBSTACLES: &[(f64, f64, f64)] = &[
        (-5.0450899771303552, 8.9500544913850071, 0.80000000000000004),
        (21.289029047055973, -4.8678002684166986, 1.2161121571549691),
        (-25.906184850856985, 11.628076549992391, 2.6113508711624895),
        (16.697961214277896, 19.852725942645652, 5.3561594737719842),
        (27.137773262157847, -12.436709145120943, 6.8426085793553426),
        (-20.411058570281668, 14.41521919222234, 2.2913859795268263),
        (15.853051744985869, 2.4560079869838796, 2.203109268225349),
        (-1.4904262460650128, -20.348157684084544, 3.537061647392056),
        (-22.89544521767462, -15.488185572124523, 3.1616040247822297),
        (2.1206728137039303, -9.2415934000673623, 0.80000000000000004),
        (3.793013347382955, 14.484756004537353, 0.80000000000000004),
        (2.6947511326485674, 3.9966689558272872, 4.2177213357002596),
        (19.147423985076365, -5.8991589430792128, 2.3223251770141018),
        (-13.884867414879761, -13.3829265178107, 4.9394491101577955),
        (23.181401627255234, 9.7836629755500137, 2.904744683492186),
        (24.842451321838979, -0.43657516700644133, 0.89375275122357101),
        (32.510227261003827, 16.965904329756899, 5.8124788067945534),
        (-25.455321861290663, 0.29487700805985007, 1.3867647370303171),
        (29.668922220488533, -1.139861769418784, 4.198637187416292),
        (-23.351557613234945, -7.8055849362409155, 1.6748248646036212),
        (-2.9313984233878196, 10.827656056048056, 4.5856958979077085),
        (24.263184751302063, 8.1858719912438573, 5.2764914440183208),
        (-17.024683070197419, -7.275429265674239, 0.80000000000000004),
        (-18.125289392058768, 16.103221990614745, 4.3675392287580657),
        (-15.361425614670171, -13.349131512540009, 1.869104942304028),
        (13.421151540254725, -16.243765347317122, 2.6311295422934315),
        (18.336103699291598, -5.9646353267033003, 4.841861928503727),
        (32.321648507950172, -1.6685262796709228, 5.1448309850835798),
    ];

    /// `1782642582` 第 0 批之前活着的四名士兵（真值）。
    const REAL_SOLDIERS: &[Target] = &[
        Target { id: 153656, x: -1.5197430999618304, y: -10.532225891038742, radius: 0.5 },
        Target { id: 153431, x: 17.449537510408554, y: 6.4740191878058901, radius: 0.5 },
        Target { id: 153430, x: 3.0996240114927369, y: -2.7150608537027026, radius: 0.5 },
        Target { id: 153655, x: -19.440003067755988, y: 3.333280297434261, radius: 0.5 },
    ];

    /// `1782642582` 第 0 批（tick 27564）的真实报文，逐条原文。
    const REAL_BATCH0: &[&str] = &[
        r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#,
        r#"{"FunctionInfo":{"owner_id":153655,"function":"","corrected_function":" + 3.333"}}"#,
        r#"{"FunctionInfo":{"owner_id":153430,"function":"-4/(1+exp(999(x-11.3)))+16/(1+(999(x-12))^2)","corrected_function":"-4/(1+exp(999(x-11.3)))+16/(1+(999(x-12))^2) + 1.285"}}"#,
        r#"{"FunctionPoints":{"owner_id":153655,"start_x":-19.44000306775599,"start_y":3.333280297434261,"end_x":-1.4962530677575945}}"#,
        r#"{"FunctionPoints":{"owner_id":153430,"start_x":3.099624011492737,"start_y":-2.7150608537027026,"end_x":-11.99959123580371}}"#,
        r#"{"GameStateInfo":{"game_state":"SynchronizingResults"}}"#,
        r#"{"FunctionActive":{"owner_id":153655,"active_tick":27659}}"#,
        r#"{"EffectInfo":{"effect_id":153688,"pos_x":-1.4962530677575945,"pos_y":3.333,"effect_type":"ExplosionEffect","start_tick":27722}}"#,
        r#"{"FunctionActive":{"owner_id":153430,"active_tick":27659}}"#,
        r#"{"EffectInfo":{"effect_id":153689,"pos_x":-11.99959123580371,"pos_y":14.998257311034074,"effect_type":"ExplosionEffect","start_tick":27772}}"#,
        r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#,
    ];

    /// `1782642582` 第 1 批（tick 30774）的真实报文，逐条原文。
    const REAL_BATCH1: &[&str] = &[
        r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#,
        r#"{"FunctionInfo":{"owner_id":153656,"function":"(7.2/4.5)x","corrected_function":"(7.2/4.5)x - 8.101"}}"#,
        r#"{"FunctionInfo":{"owner_id":153431,"function":"(-6.474)*(1/((1+exp(-42*(x-(-10))))*(1+exp(42*(x-(25))))))+(-12)*(1/((1+exp(-42*(x-(2))))*(1+exp(42*(x-(5))))))+(40)*(1/((1+exp(-42*(x-(19.5))))*(1+exp(42*(x-(25))))))","corrected_function":"(-6.474)*(1/((1+exp(-42*(x-(-10))))*(1+exp(42*(x-(25))))))+(-12)*(1/((1+exp(-42*(x-(2))))*(1+exp(42*(x-(5))))))+(40)*(1/((1+exp(-42*(x-(19.5))))*(1+exp(42*(x-(25)))))) + 6.474"}}"#,
        r#"{"FunctionPoints":{"owner_id":153656,"start_x":-1.5197430999618304,"start_y":-10.532225891038742,"end_x":5.595256900038063}}"#,
        r#"{"FunctionPoints":{"owner_id":153431,"start_x":17.449537510408554,"start_y":6.47401918780589,"end_x":4.242603916659716}}"#,
        r#"{"GameStateInfo":{"game_state":"SynchronizingResults"}}"#,
        r#"{"EffectInfo":{"effect_id":153874,"pos_x":3.099624011492737,"pos_y":-2.7150608537027026,"effect_type":"DeathEffect","start_tick":30894}}"#,
        r#"{"FunctionActive":{"owner_id":153656,"active_tick":30864}}"#,
        r#"{"EffectInfo":{"effect_id":153875,"pos_x":5.595256900038063,"pos_y":0.8514110400608992,"effect_type":"ExplosionEffect","start_tick":30911}}"#,
        r#"{"FunctionActive":{"owner_id":153431,"active_tick":30864}}"#,
        r#"{"EffectInfo":{"effect_id":153876,"pos_x":4.242603916659716,"pos_y":0.0,"effect_type":"ExplosionEffect","start_tick":30931}}"#,
        r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#,
    ];

    /// `1782640359` 第 0 批（tick 3422）的真实报文，逐条原文（单发；靶子 350 皮肤 `target`
    /// ⇒ 死亡特效是 `SparkEffect`）。
    const REAL_SINGLE_SHOT: &[&str] = &[
        r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#,
        r#"{"FunctionInfo":{"owner_id":348,"function":"(40)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(25))))))","corrected_function":"(40)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(25)))))) - 0.297"}}"#,
        r#"{"FunctionPoints":{"owner_id":348,"start_x":-2.1413650290582837,"start_y":-0.29656732812543574,"end_x":4.988566611566738}}"#,
        r#"{"GameStateInfo":{"game_state":"SynchronizingResults"}}"#,
        r#"{"EffectInfo":{"effect_id":352,"pos_x":5.030674058137304,"pos_y":1.6580370528335653,"effect_type":"SparkEffect","start_tick":3515}}"#,
        r#"{"FunctionActive":{"owner_id":348,"active_tick":3486}}"#,
        r#"{"EffectInfo":{"effect_id":353,"pos_x":4.988566611566738,"pos_y":14.991174112697315,"effect_type":"ExplosionEffect","start_tick":3561}}"#,
        r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#,
    ];

    /// 上面三条真批测试里不再用到的小样例构造器（保留给别的测试与人工排查）。
    #[allow(dead_code)]
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

    /// 我方报文**投影**成"回放会记的那些 kind"（按 [`Policy::unobserved_kinds`] 去掉
    /// 回放记录域里不收的种类，例如 `QueueDestroyObstacle`），`EffectInfo` 展开成
    /// `EffectInfo/<effect_type>`。投影口径与 `examples/batch_conv.rs` 的
    /// `project_ours` 同源，所以种类序列可以直接与回放原文逐项比。
    fn projected_kinds(policy: &Policy, wire: &[String]) -> Vec<String> {
        wire.iter()
            .map(|w| Msg::parse_wire(w).expect("我方 emitter 产出了非法 JSON"))
            .filter(|m| !policy.is_unobserved(m))
            .map(|m| {
                if m.kind == "EffectInfo" {
                    format!("EffectInfo/{}", m.get_str("effect_type").unwrap_or_default())
                } else {
                    m.kind
                }
            })
            .collect()
    }

    /// **单发批**：`fire` ≡ `fire_batch(N=1)`，而且两条路径都必须**复现真回放里那一批**。
    ///
    /// ## 真值来源（观测，冻结语料）
    ///
    /// 真值 = replay `1782640359` 的第 0 个结算批（tick 3422，单发）。
    /// 该文件在冻结快照 `.reference/replays/gw2-replays-snapshot/replays/` 里，
    /// sha256 **`b4615ff7267f9ab657d9bc1907cb2a36ca28622951e4282764c12901898a75d2`**
    /// （逐份 sha256 同时登记在 `fixtures/guardrails.json` 的 `corpus`）。
    /// 起点态由 `tools/replay_state.py <snapshot>/1782640359 --index 0` 复算得
    /// （0 个障碍；士兵 348 `(-2.1413650290582837, -0.29656732812543574)`、
    /// 靶子 350 `(5.030674058137304, 1.6580370528335653)`，皮肤 `target` ⇒ `SparkEffect`）。
    /// 报文逐条抄自回放原文（见下方 `REAL_SINGLE_SHOT`），比较用**同一套**
    /// `compare_batch` + `strict_policy()`（墙钟 tick / effect_id 计数器按既有口径不比）。
    ///
    /// 改前这条只比"我方两条路径彼此相等" ⇒ 两条路径**一起错也绿**。
    #[test]
    fn single_shot_is_byte_identical_to_fire_batch() {
        let mk = || {
            let mut q = ConvPredictor::new(
                &[],
                vec![
                    Target { id: 348, x: -2.1413650290582837, y: -0.29656732812543574, radius: 0.5 },
                    Target { id: 350, x: 5.0306740581373042, y: 1.6580370528335653, radius: 0.5 },
                ],
                Mode::Normal,
            );
            q.skin.insert(350, "target".to_string());
            q
        };
        let text = "(40)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(25))))))";
        // ① N=1：两条路径逐字节相同
        let (a, oa) = mk().fire(348, text, -2.1413650290582837, -0.29656732812543574, false, 3422);
        let shot = ShotInput {
            owner: 348,
            x0: -2.1413650290582837,
            y0: -0.29656732812543574,
            flip: false,
            text: text.into(),
        };
        let (b, ob) = mk().fire_batch(&[shot], 3422);
        assert_eq!(a, b, "N=1 的报文必须逐字节相同");
        assert_eq!(oa.stop, ob[0].stop);
        assert_eq!(oa.killed, ob[0].killed);
        assert_eq!(oa.corrected, ob[0].corrected);

        // ② ★ 真值：两条路径都必须复现真回放那一批
        let observed: Vec<Msg> = REAL_SINGLE_SHOT
            .iter()
            .map(|l| Msg::parse_wire(l).expect("真报文应能解析"))
            .collect();
        for (tag, wire) in [("fire", &a), ("fire_batch", &b)] {
            let ours: Vec<Msg> = wire
                .iter()
                .map(|w| Msg::parse_wire(w).expect("我方 emitter 产出了非法 JSON"))
                .collect();
            let v = compare_batch(&ours, &observed, &strict_policy(), 30);
            assert!(
                v.kind_seq_ok && v.content_ok && v.order_ok,
                "{tag} 没有复现 replay 1782640359 的第 0 批（kind_seq={} content={} order={} diffs={:?}）",
                v.kind_seq_ok,
                v.content_ok,
                v.order_ok,
                v.diffs.iter().map(|d| format!("{} ours={} obs={}", d.path, d.ours, d.observed)).collect::<Vec<_>>()
            );
        }
        // 轨迹末点（FunctionPoints.end_x）也要是真回放里那个值的位
        let end_x_bits = ob[0].stop.0.to_bits();
        assert_eq!(end_x_bits, 4.988566611566738_f64.to_bits(), "end_x 与真回放不一致");
    }

    /// 两发一批：**前缀按类型分组、后缀每发一对** —— 真值 = 冻结语料里一个真实的两发批。
    ///
    /// ## 真值来源（观测，冻结语料）
    ///
    /// 真值 = replay `1782642582` 的第 0 个结算批（tick 27564，两发）。
    /// 快照文件 sha256 **`fbed66caf5dbc2610a28776176366da0a22816f67d47c3e218c722f408d598e9`**。
    /// 起点态：`tools/replay_state.py <snapshot>/1782642582 --index 0`
    /// → 28 个障碍（`REAL_OBSTACLES`）、四名士兵（`REAL_SOLDIERS`）。
    /// 两发分别是 owner 153655（Team1，空函数）与 153430（Team2，反向）。
    ///
    /// 改前这条的期望序列是"照我方模型 + `docs/141` §3.1 抄的" ⇒ 我方模型错则期望一起错；
    /// 现在期望**逐条来自回放原文**，且用同一套 `compare_batch` + `strict_policy()` 比。
    #[test]
    fn two_shot_batch_is_grouped_by_type() {
        let mut p = ConvPredictor::new(REAL_OBSTACLES, REAL_SOLDIERS.to_vec(), Mode::Normal);
        let shots = vec![
            ShotInput {
                owner: 153655,
                x0: -19.44000306775599,
                y0: 3.333280297434261,
                flip: false,
                text: String::new(),
            },
            ShotInput {
                owner: 153430,
                x0: 3.099624011492737,
                y0: -2.7150608537027026,
                flip: true,
                text: "-4/(1+exp(999(x-11.3)))+16/(1+(999(x-12))^2)".into(),
            },
        ];
        let (wire, outs) = p.fire_batch(&shots, 27564);
        assert_eq!(outs.len(), 2);

        // 我方种类序列（含 effect_type 细分，投影掉回放不记的 `QueueDestroyObstacle`）
        let policy = strict_policy();
        let our_kinds = projected_kinds(&policy, &wire);
        assert_eq!(
            our_kinds,
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
            "= 真回放 1782642582 第 0 批的分组（前缀按类型连排、后缀每发一对）"
        );

        // ★ 真值：逐字段与真回放那一批比
        let observed: Vec<Msg> = REAL_BATCH0
            .iter()
            .map(|l| Msg::parse_wire(l).expect("真报文应能解析"))
            .collect();
        let ours: Vec<Msg> = wire
            .iter()
            .map(|w| Msg::parse_wire(w).expect("我方 emitter 产出了非法 JSON"))
            .collect();
        let v = compare_batch(&ours, &observed, &policy, 30);
        assert!(
            v.kind_seq_ok && v.content_ok && v.order_ok,
            "没有复现 replay 1782642582 第 0 批（kind_seq={} content={} order={} diffs={:?}）",
            v.kind_seq_ok,
            v.content_ok,
            v.order_ok,
            v.diffs.iter().map(|d| format!("{} ours={} obs={}", d.path, d.ours, d.observed)).collect::<Vec<_>>()
        );
        // 两发的 end_x 都要是回放里那两位
        assert_eq!(outs[0].stop.0.to_bits(), (-1.4962530677575945_f64).to_bits());
        assert_eq!(outs[1].stop.0.to_bits(), (-11.99959123580371_f64).to_bits());
    }


    /// 死亡块**按飞行时间**排、同一士兵只出现一次 —— 真值 = 冻结语料里一个真实的两发批。
    ///
    /// ## 真值来源（观测，冻结语料）
    ///
    /// 真值 = replay `1782642582` 的**第 1 个**结算批（tick 30774，两发，含一条 `DeathEffect`）。
    /// 快照 sha256 与第 0 批同一份文件（`fbed66ca…`，见
    /// [`two_shot_batch_is_grouped_by_type`]）。状态是**从第 0 批接着喂**出来的
    /// （预测器自带状态：掩膜上已被挖的洞、已死的士兵都在里面），所以这条测试也顺带
    /// 钉住"跨批状态一致"。
    ///
    /// 真回放里这两发的死亡块位置：
    /// `DeathEffect(pos_x=3.099624011492737, pos_y=-2.7150608537027026)`（= 士兵 153430 的
    /// **批开始**位置）在 `FunctionActive` 之前，属"前缀按类型连着排"。
    ///
    /// 改前这条的期望值与几何（谁先死、死在哪）全是照我方模型编的。
    #[test]
    fn death_block_follows_flight_time() {
        let mut p = ConvPredictor::new(REAL_OBSTACLES, REAL_SOLDIERS.to_vec(), Mode::Normal);
        // 第 0 批（同 [`two_shot_batch_is_grouped_by_type`]）
        let b0 = vec![
            ShotInput {
                owner: 153655,
                x0: -19.44000306775599,
                y0: 3.333280297434261,
                flip: false,
                text: String::new(),
            },
            ShotInput {
                owner: 153430,
                x0: 3.099624011492737,
                y0: -2.7150608537027026,
                flip: true,
                text: "-4/(1+exp(999(x-11.3)))+16/(1+(999(x-12))^2)".into(),
            },
        ];
        let (w0, _) = p.fire_batch(&b0, 27564);
        let obs0: Vec<Msg> = REAL_BATCH0.iter().map(|l| Msg::parse_wire(l).unwrap()).collect();
        let ours0: Vec<Msg> = w0.iter().map(|w| Msg::parse_wire(w).unwrap()).collect();
        assert!(compare_batch(&ours0, &obs0, &strict_policy(), 30).content_ok, "第 0 批先要能复现");

        // 第 1 批：真回放里 153430 被 153431 那一发打死
        let b1 = vec![
            ShotInput {
                owner: 153656,
                x0: -1.5197430999618304,
                y0: -10.532225891038742,
                flip: false,
                text: "(7.2/4.5)x".into(),
            },
            ShotInput {
                owner: 153431,
                x0: 17.449537510408554,
                y0: 6.4740191878058901,
                flip: true,
                text: "(-6.474)*(1/((1+exp(-42*(x-(-10))))*(1+exp(42*(x-(25))))))+(-12)*(1/((1+exp(-42*(x-(2))))*(1+exp(42*(x-(5))))))+(40)*(1/((1+exp(-42*(x-(19.5))))*(1+exp(42*(x-(25))))))".into(),
            },
        ];
        let (wire, outs) = p.fire_batch(&b1, 30774);
        let observed: Vec<Msg> = REAL_BATCH1.iter().map(|l| Msg::parse_wire(l).unwrap()).collect();
        let ours: Vec<Msg> = wire.iter().map(|w| Msg::parse_wire(w).unwrap()).collect();
        let v = compare_batch(&ours, &observed, &strict_policy(), 30);
        assert!(
            v.kind_seq_ok && v.content_ok && v.order_ok,
            "没有复现 replay 1782642582 第 1 批（kind_seq={} content={} order={} diffs={:?}）",
            v.kind_seq_ok,
            v.content_ok,
            v.order_ok,
            v.diffs.iter().map(|d| format!("{} ours={} obs={}", d.path, d.ours, d.observed)).collect::<Vec<_>>()
        );
        // 真回放只记一条 `DeathEffect`，**不记是哪一发打死的** ⇒ 只断言"死的集合"，
        // 不去断言"第几发"（那是我方模型内部的概念，回放里没有真值）。
        let mut killed_union: Vec<i64> = outs.iter().flat_map(|o| o.killed.clone()).collect();
        killed_union.sort_unstable();
        killed_union.dedup();
        assert_eq!(killed_union, vec![153430], "真回放这一批只有 153430 死");
        let death_effects: Vec<(f64, f64)> = ours
            .iter()
            .filter(|m| m.kind == "EffectInfo" && m.get_str("effect_type").as_deref() == Some("DeathEffect"))
            .map(|m| (m.get_f64("pos_x").unwrap_or(f64::NAN), m.get_f64("pos_y").unwrap_or(f64::NAN)))
            .collect();
        assert_eq!(death_effects.len(), 1, "真回放这一批只有一条 DeathEffect");
        assert_eq!(death_effects[0].0, 3.099624011492737, "死者位置来自批开始快照");
        assert_eq!(death_effects[0].1, -2.7150608537027026);
        // 状态落地：153430 从士兵表里移除（真回放后续 `LifeInfo alive:false`）
        assert_eq!(p.soldiers.len(), 3);
        assert!(p.soldiers.iter().all(|t| t.id != 153430));
    }

    // -----------------------------------------------------------------------
    // 批内结算顺序（S3）：收敛层与内核**同一个函数**（`docs/06` §42）
    // -----------------------------------------------------------------------

    /// 内核测试用的那间房（`RoomState` 只是 `Match::new` 的入参，几何用不到）。
    fn room() -> crate::server::RoomState {
        crate::server::RoomState {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        }
    }

    /// **收敛层真的按 S3 结算，而且与内核逐位相同**（`docs/06` §42）。
    ///
    /// 几何照抄 `game.rs::tests::arrival_order_makes_earlier_holes_visible`（同一堵墙、
    /// 同一组起点）：一堵竖墙在 `x = 0`（半径 1 的圆盘沿 y 排开），两发都从右边水平
    /// 向左打。`owner=1` 从 `(10, 0)` 出发（**先到**），`owner=2` 从 `(12, -0.1)` 出发
    /// （**后到**，路径从 1 号挖的洞里穿过），报文序故意给成 `[2, 1]`。
    ///
    /// 三条断言各钉一件事，缺一条这个测试就会变成空转：
    ///
    /// 1. **S1 ≠ S3**：改档位真的改了收敛层的几何（否则"两层都调同一个函数"是句空话）；
    /// 2. **与内核 `fire_functions_with(S3)` 逐位相同**：点列 / 停点 / `blocked` /
    ///    `killed` / `hits` 全比 —— 同构不是"看起来一样"，是同一个函数的两个驱动器；
    /// 3. **报文跟着重解后的停点走**：`FunctionPoints.end_x` = 第二遍的 `stop.x`
    ///    （第一遍那个是错的），否则报文和几何会各说一套。
    #[test]
    fn arrival_order_follows_the_kernel_bit_for_bit() {
        // 墙：x = 0 上从 y=-3 到 3 排一串半径 1 的圆盘（与内核那条测试同一个几何）
        let mut obstacles = Vec::new();
        let mut y = -3.0;
        while y <= 3.0 {
            obstacles.push((0.0, y, 1.0));
            y += 0.25;
        }
        let shots = vec![
            ShotInput { owner: 2, x0: 12.0, y0: -0.1, flip: true, text: "0".into() },
            ShotInput { owner: 1, x0: 10.0, y0: 0.0, flip: true, text: "0".into() },
        ];
        let pred = || ConvPredictor::new(&obstacles, Vec::new(), Mode::Normal);
        let (_w1, s1) = pred().fire_batch_with(&shots, 0, crate::game::SettleOrder::S1Message);
        let (w3, s3) =
            pred().fire_batch_with(&shots, 0, crate::game::SettleOrder::S3ArrivalHoles);

        // ① 两发都撞墙（否则这个几何什么都没量到）；先到的那发不受影响，后到的看得见洞
        assert!(s1[0].blocked && s1[1].blocked, "两发都该撞墙：{:?}", (s1[0].blocked, s1[1].blocked));
        assert_eq!(s3[1].stop, s1[1].stop, "先到的那一发不受影响");
        assert_ne!(s3[0].stop, s1[0].stop, "后到的那一发必须看得见前面的洞");
        assert!(
            s3[0].stop.0 < s1[0].stop.0 - 1e-9,
            "应当穿过洞继续往左：S1 x={} / S3 x={}",
            s1[0].stop.0,
            s3[0].stop.0
        );

        // ② 与内核逐位相同（两边都调 `game::settle_in_order_with_holes`）
        let mut kernel = crate::game::Match::new(room(), obstacles.clone(), Vec::new(), (0, 0));
        let subs: Vec<crate::game::Submission<'_>> = shots
            .iter()
            .map(|s| crate::game::Submission {
                owner_id: s.owner,
                function_text: &s.text,
                x0: s.x0,
                y0: s.y0,
                flip: s.flip,
            })
            .collect();
        let ko = kernel.fire_functions_with(&subs, crate::game::SettleOrder::S3ArrivalHoles);
        for i in 0..shots.len() {
            assert_eq!(bits_pair(ko[i].stop), bits_pair(s3[i].stop), "第 {i} 发停点");
            assert_eq!(ko[i].blocked, s3[i].blocked, "第 {i} 发 blocked");
            assert_eq!(ko[i].killed, s3[i].killed, "第 {i} 发 killed");
            assert_eq!(ko[i].points.len(), s3[i].points.len(), "第 {i} 发点数");
            assert!(
                ko[i]
                    .points
                    .iter()
                    .zip(s3[i].points.iter())
                    .all(|(a, b)| bits_pair(*a) == bits_pair(*b)),
                "第 {i} 发点列必须逐位相同"
            );
            assert_eq!(ko[i].hits.len(), s3[i].hits.len(), "第 {i} 发命中数");
            assert!(
                ko[i]
                    .hits
                    .iter()
                    .zip(s3[i].hits.iter())
                    .all(|(a, b)| a.id == b.id && a.index == b.index && a.arc.to_bits() == b.arc.to_bits()),
                "第 {i} 发命中细节（含弧长）必须逐位相同"
            );
        }

        // ③ 报文里的 `end_x` 是**第二遍**的停点
        let ends: Vec<f64> = w3
            .iter()
            .filter_map(|w| Msg::parse_wire(w).ok())
            .filter(|m| m.kind == "FunctionPoints")
            .filter_map(|m| m.get_f64("end_x"))
            .collect();
        assert_eq!(ends.len(), 2, "两发各一条 `FunctionPoints`");
        assert_eq!(ends[0].to_bits(), s3[0].stop.0.to_bits(), "end_x 必须跟重解后的停点");
        assert_ne!(ends[0].to_bits(), s1[0].stop.0.to_bits(), "不能是第二遍之前那个值");

        // ---- T59 真值：真机批里"先落命中、再落到达"的顺序（299 批对 0 批）----
        assert_eq!(REAL_DEATH_BEFORE_EXPLOSION, (299, 0));
    }

    // -----------------------------------------------------------------------
    // 坐标并回内核（`docs/06` §30）：机械等价性 + "没有第二份规则"
    // -----------------------------------------------------------------------

    /// **机械等价性**：`explosion_point(corrected_function(t), stop, …)` ≡ `game::hole_point(false, t, stop, …)`。
    ///
    /// 这是"爆炸点规则只有内核一份"的证据：本模块那个旧签名助手（吃**已印出**的文本）
    /// 在同一发的**真实停点**上必须与内核逐位一致 —— 它自己再写一遍规则的话，这条断言
    /// 就是它的警戒线。语料侧的同一件事见 `examples/diffconv_parity`（740 发，§30.5）。
    ///
    /// 停点用**这一发自己采样出来的**（`shot_points` 的末点），不是随便编的坐标：
    /// 空函数（原文 `""`）那一档两者会分家 —— `hole_point` 在"原文解析为空"时**提前返回
    /// 停点**，而 `explosion_point` 会去求值印出来的 `" + c.ddd"`。生产的停点恰好等于
    /// 那个常数，所以管线里永远同值；拿一个不相干的停点去比才看得出这条早退。
    #[test]
    fn explosion_point_is_the_kernel_hole_point() {
        // 均衡 / 缺括号（11 条那一类）/ 多余右括号（8 条那一类）/ flip / 空函数 / 非有限
        let cases = [
            ("(40)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(25))))))", -2.1413650290582837, -0.29656732812543574, false),
            ("5/155sin(4exp(4*6cos(", -2.632134919326489, 6.05141758173637, false),
            ("14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2))", 22.908325288230134, -4.32330978206142, true),
            ("sin(x)", 14.559477538378344, -6.7280811727617085, true),
            ("", 0.0, 3.0, false),
            ("0.00", 1.651220991201045, 4.638020825690482, false),
            ("1/(0)", 0.0, 2.0, false),
        ];
        for (text, x0, y0, flip) in cases {
            let printed = corrected_function(text, x0, y0, flip, Mode::Normal);
            // 这一发自己的停点（与 `ConvPredictor::fire_batch` 同一个内核入口）
            let samp = sampling_text(text, x0, y0, flip, Mode::Normal);
            let pts = crate::graphwar_shot_calculator::shot_points(
                &samp, x0, y0, crate::SERVER_STEP, flip, Mode::Normal, false,
            );
            let stop = *pts.last().expect("弹道不该是空的");
            let ours = explosion_point(&printed, stop, flip, Mode::Normal);
            let kern = crate::game::hole_point(false, text, x0, y0, flip, stop);
            assert_eq!(ours.0.to_bits(), kern.0.to_bits(), "x 位不同：text={text:?} flip={flip}");
            assert_eq!(
                ours.1.to_bits(),
                kern.1.to_bits(),
                "y 位不同：text={text:?} flip={flip} stop={stop:?}（printed={printed:?}）"
            );
        }
        // DiffEq：两边都**不重算**，直接回停点
        let stop = (1.5, -2.5);
        assert_eq!(explosion_point("whatever", stop, true, Mode::Ode), stop);
        assert_eq!(crate::game::hole_point(true, "whatever", 0.0, 0.0, true, stop), stop);
        // T59 真值：真语料 1291 条 `FunctionPoints` 里 1050 条 `ExplosionEffect.pos_x` 逐位相同、
        // 1179 条在 1e-4 内 ⇒ "爆炸点由停点决定"这条规则有真报文背书
        assert_eq!(REAL_EXPLOSION_AT_END, (1050, 1179, 1291));
    }

    /// `sampling_text` 就是内核那条规则：`Normal` 与 `game::sampling_function_text` 逐字节相同，
    /// `Ode` 是原文（内核没有 Ode 开火路径，见 §30.4）。
    #[test]
    fn sampling_text_is_the_kernel_rule() {
        let f = "5/155sin(4exp(4*6cos(";
        let (x0, y0) = (-2.632134919326489, 6.05141758173637);
        assert_eq!(
            sampling_text(f, x0, y0, false, Mode::Normal),
            crate::game::sampling_function_text(f, x0, y0, false)
        );
        // 多余右括号：采样串要去掉（否则我方 parse 截断、常数被丢掉，弹道退化成 1 个点）
        let g = "14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2))";
        let samp = sampling_text(g, 22.908325288230134, -4.32330978206142, true, Mode::Normal);
        assert!(!samp.contains(")))+24"), "采样串里不该有多余的 `)`：{samp:?}");
        // 发出去的串只补不删 —— 两者不是同一个串
        let printed = corrected_function(g, 22.908325288230134, -4.32330978206142, true, Mode::Normal);
        assert!(printed.starts_with(g), "报文串必须逐字符保留原文：{printed:?}");
        // DiffEq：采样串 = 原文（一个字符都不加）
        assert_eq!(sampling_text(f, x0, y0, false, Mode::Ode), f);
        // T59 真值：真语料 19 条括号不平衡的**定稿**报文，采样串（去掉多余右括号那一步）
        // 在真报文里的体现 = 定稿串以原文开头且括号平衡（见 REAL_UNBALANCED）
        assert_eq!(REAL_UNBALANCED[0].0, f, "真语料里就是这条原文（1782642986）");
    }

    /// 射手排除由**内核**做（只比 id，不比位置）：站在射手脚下（距离 0）的另一名士兵
    /// 也会被打死 —— 旧实现在调用方多写了一条 `1e-12` 位置排除，二进制没有这条。
    #[test]
    fn owner_exclusion_is_the_kernels_not_prose() {
        let mut p = ConvPredictor::new(
            &[],
            vec![
                Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 },
                Target { id: 2, x: 0.0, y: 0.0, radius: 0.5 },
            ],
            Mode::Normal,
        );
        let (_, outs) = p.fire_batch(
            &[ShotInput { owner: 1, x0: 0.0, y0: 0.0, flip: false, text: "0.00".into() }],
            100,
        );
        // 1 号（射手）不算；2 号与射手同坐标 ⇒ 照样死（内核只比 id）
        assert_eq!(outs[0].killed, vec![2], "同坐标的非射手必须被判死");
        // T59 真值：真语料 950 个单射手批里，射手 id 出现在死亡集合的次数是 0
        assert_eq!(REAL_SINGLE_SHOOTER_SELF_KILL, (950, 0));
    }
}
