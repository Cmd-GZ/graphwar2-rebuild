//! 一次开火的完整结算：弹道 + 停止点 + 致死。
//!
//! 模型来自真实对局抓包（见 `docs/25`、`docs/30`）：
//!
//! * **障碍**决定弹道在哪里停：把障碍光栅化进掩膜，沿轨迹逐点查；
//! * **士兵**不挡弹道，只在弹道**经过其半径内**时被判定致死；
//! * 因此两者**不能**混在同一张掩膜里——这是 `docs/30` 纠正过的错误建模。
//!
//! 调用方负责：
//!
//! * 传入只含障碍的掩膜（`plane::empty_mask()` + 逐个 `rasterize`）；
//! * 传入**除射手本人以外**的士兵（射手不会打死自己，这一点由调用方保证）。

use crate::graphwar_path_calculator::Mode;
use crate::plane::is_occupied;
use crate::graphwar_shot_calculator::shot_points;

/// 参与结算的士兵。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub id: i64,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

/// 一次命中：谁、在弹道的哪个位置被扫过。
///
/// 存在的唯一理由：同时回合制要按**弹丸飞行时间**给一批死亡排序（`docs/86` §6、
/// `docs/141`）。同一回合里所有弹丸的 `active_tick` 相同（13/13 回合观测），
/// 所以"谁先到"等价于"谁的弧长更短" —— 不需要先知道那 ≈0.29 单位/tick 的速度本身。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// 被扫过的士兵 id。
    pub id: i64,
    /// 从起点沿弹道量到命中点的弧长（棋盘单位）。
    pub arc: f64,
    /// 命中发生在第几个采样点（0 = 起点）。
    pub index: usize,
}

/// 一次开火的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ShotOutcome {
    /// 弹道点序列（含起点）。
    pub points: Vec<(f64, f64)>,
    /// 弹道停止处的点（撞到障碍或到达边界）。
    pub stop: (f64, f64),
    /// 是否因撞到障碍而停止（`false` 表示飞到边界/点用尽）。
    pub blocked: bool,
    /// 被判定致死的士兵，按**经过的先后顺序**。
    pub killed: Vec<i64>,
    /// 与 `killed` 一一对应的命中细节（弧长 + 采样点下标），顺序相同。
    pub hits: Vec<Hit>,
}

/// 结算一次开火。
///
/// `mask` 只应包含障碍；`targets` 不含射手本人。
pub fn resolve(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    mask: &[u8],
    targets: &[Target],
) -> ShotOutcome {
    let points = shot_points(text, x0, y0, step, flip, mode, true);
    resolve_points(mask, targets, &points)
}

/// 带"射手"的 [`resolve`]：见 [`resolve_points_owned`]。
#[allow(clippy::too_many_arguments)]
pub fn resolve_owned(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    mask: &[u8],
    targets: &[Target],
    owner: Option<i64>,
) -> ShotOutcome {
    let points = shot_points(text, x0, y0, step, flip, mode, true);
    resolve_points_owned(mask, targets, &points, owner)
}

/// 与 [`resolve`] 同一套规则，但**点序列由调用方给**（`docs/120`）。
///
/// 存在的理由只有一个：GDScript 那边已经拿着内核算好的点序列
/// （`path_calculate` 的返回值），没必要为了判命中再算一遍 ——
/// 更重要的是**不能**再算一遍：两次采样的输入只要有一点差别，客户端画的弹道
/// 和致死判定就会分家。规则本体只有这一份（[`resolve`] 也走这里）。
///
/// 规则逐条与 [`resolve`] 相同（`docs/30`）：
///
/// 1. 每个点先判致死（`d <= radius`），再判阻挡（掩膜 `!= 0xFF`）；
/// 2. 士兵**不**影响弹道，只有障碍让弹道停下；
/// 3. 同一士兵只记一次；`killed` 的顺序 = 弹道经过的先后。
///
/// `points` 用**棋盘坐标**，`mask` 是**像素空间**的 1000×600 掩膜；
/// 两者之间的换算在 [`crate::plane::is_occupied`] 里（`px = 20x+500`），
/// 本函数与调用方都**不要**再翻一次（`docs/120` §2）。
pub fn resolve_points(mask: &[u8], targets: &[Target], points: &[(f64, f64)]) -> ShotOutcome {
    resolve_points_owned(mask, targets, points, None)
}

/// 带"射手"的 [`resolve_points`]：`owner` 那个实体**不参与致死判定**。
///
/// 这是内核里原本缺失的那条规则（`docs/122` §4）。二进制在
/// **`0x3c4452`（有弹道点）与 `0x3c3e81`（空弹道）** 两处比较的都是开火记录
/// `+0x18` 的射手 id，**没有**"位置与起点重合"这种判据 ⇒ 这里也只比 id。
///
/// `None` = 不排除任何人（`api::hit_test` 的既有契约就是这样，16 条测试不动）。
pub fn resolve_points_owned(
    mask: &[u8],
    targets: &[Target],
    points: &[(f64, f64)],
    owner: Option<i64>,
) -> ShotOutcome {
    let (x0, y0) = *points.first().unwrap_or(&(0.0, 0.0));
    let mut killed: Vec<i64> = Vec::new();
    let mut hits: Vec<Hit> = Vec::new();
    let mut stop = (x0, y0);
    let mut blocked = false;
    // 沿弹道累计的弧长：同时回合制靠它给一批死亡排序（`docs/141` §2）。
    let mut arc = 0.0f64;

    for (index, &(px, py)) in points.iter().enumerate() {
        if index > 0 {
            let (qx, qy) = points[index - 1];
            arc += ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
        }
        // 先判致死：经过即死，且不影响弹道
        for t in targets {
            if killed.contains(&t.id) {
                continue;
            }
            // 射手本人不参与致死判定（`0x3c4456`）：只比 id，不比位置。
            if Some(t.id) == owner {
                continue;
            }
            let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
            if d <= t.radius {
                killed.push(t.id);
                hits.push(Hit { id: t.id, arc, index });
            }
        }
        // 再判阻挡：只有障碍会让弹道停下
        if is_occupied(mask, px, py) {
            stop = (px, py);
            blocked = true;
            break;
        }
        stop = (px, py);
    }

    ShotOutcome { points: points.to_vec(), stop, blocked, killed, hits }
}

// ===========================================================================
// 坐标计算：**唯一**一份实现（`docs/06-verification.md` §30）
// ===========================================================================
//
// 这一节把"函数原文 → 报文里所有坐标"的规则收成**一个入口**：
//
// ```text
// 原文 ──corrected_text──▶ (发出串, 采样串)
//                             │              │
//                             │              └─shot_points─▶ 点序列
//                             │                                  │
//                             │                        resolve_points_owned
//                             │                                  ▼
//                             └─────explosion_point──────  stop / blocked / killed / hits
//                                                        （爆炸点 = 挖洞点）
// ```
//
// **为什么要收**（工程债，`docs/142` §5 点名的"跨任务结构债"）：
// `game::Match` 与 `_scaffold::diffconv::ConvPredictor` 曾经各写一份坐标计算，
// 于是同一个分歧要先猜"是驱动错了还是内核错了"。现在驱动层只剩"驱动"：
// 它调 [`resolve_shot`]，自己**不碰任何算式**。
//
// 两处规则各自的证据在 `docs/06-verification.md` §25（修正文本按 3 位小数印出来
// 再解析回去采样）与 §29（多出来的 `)` 要去掉、缺的要补齐）。本节的函数**不改**
// 既有 [`resolve`] / [`resolve_points_owned`] 的语义，只是把"两串文本 + 采样 + 爆炸点"
// 这条流水线也变成内核的一部分（`只加不改`）。

/// 一条函数原文对应的**两串**修正文本（`docs/06-verification.md` §29）。
///
/// 为什么必须是两串：真值发出去的 `corrected_function` 是 `原文 + ")"×缺 + " ± c.ddd"`，
/// **只补不删**；而采样器吃的那一串必须把**没有配对**的 `)` 去掉 —— 我方
/// `math_function::parse` 遇到它会在原地截断，把追加的修正常数一起丢掉。
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectedText {
    /// 报文里发出去的那一串（`FunctionInfo.corrected_function`）。
    pub emitted: String,
    /// 采样器吃的那一串（喂给 [`crate::graphwar_shot_calculator::shot_points`]，不修正）。
    pub sampled: String,
}

/// 括号缺口：`(` 减 `)`，下限 0（多出来的 `)` 记成 0）。
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

/// 括号的两种处理：`(只补不删, 去掉没有配对的 ) 再补齐)`。
fn balanced_texts(text: &str) -> (String, String) {
    let mut depth = 0i64;
    let mut stripped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '(' => {
                depth += 1;
                stripped.push(ch);
            }
            ')' => {
                if depth > 0 {
                    depth -= 1;
                    stripped.push(ch);
                }
                // depth == 0 ⇒ 这条 `)` 没有配对：真解析器当噪声跳过，
                // 我方 `parse` 会在这里截断（`docs/06` §29）。
            }
            _ => stripped.push(ch),
        }
    }
    let mut closed = String::from(text);
    let mut samplable = stripped;
    for _ in 0..depth {
        closed.push(')');
        samplable.push(')');
    }
    (closed, samplable)
}

/// 修正文本（两串）。
///
/// * `Mode::Ode`（DiffEq 局）：`corrected_function == function`，**不加后缀**
///   —— 回放 550/550 零例外，所以两串都等于原文；
/// * `Mode::Normal`：`to_text(parse(function)) + " ± c.ddd"`，其中 `c = y0 - f(±x0)`
///   按 **3 位小数**印出来，**采样器吃的就是印出来的文本**（`docs/06` §25：
///   这是 `end_x` 从 27/739 修到 605/739 的那一条）。
pub fn corrected_text(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode) -> CorrectedText {
    match mode {
        Mode::Ode => CorrectedText {
            emitted: text.to_string(),
            sampled: text.to_string(),
        },
        Mode::Normal => {
            let (closed, samplable) = balanced_texts(text);
            // `c` 由**补完括号、去掉多余 `)`** 的树算：真解析器看的就是整条表达式。
            let suffix = crate::graphwar_shot_calculator::correction_text(
                &crate::math_function::parse(&samplable),
                x0,
                y0,
                flip,
            );
            CorrectedText {
                emitted: format!("{closed}{suffix}"),
                sampled: format!("{samplable}{suffix}"),
            }
        }
    }
}

/// 爆炸点（= 挖洞点）：拿**发出去的**那一串在 `x = ±stop.x` 处再求一次值。
///
/// 证据（`docs/06` §24、`docs/14` §12）：回放里 503/503 条 `DestroyObstacle` 的坐标与
/// 同一批 `EffectInfo(ExplosionEffect)` **逐位相同**；而它与采样停点的 y 最多差 ~5e-4，
/// 因为停点用**原文**采样，爆炸点用**印刷版修正文本**（常数只有 3 位小数）回代。
///
/// * `Mode::Ode`：`corrected == function`，回代无意义，爆炸点就是停点；
/// * 非有限值（NaN / ±inf）**一律回退到停点**：`±inf` 写进报文是 `"pos_y":-inf`，
///   不是合法 JSON，下游整条丢掉（`docs/06` §25.7）。回退值本身**不是**真值。
pub fn explosion_point(
    emitted: &str,
    stop: (f64, f64),
    flip: bool,
    mode: Mode,
) -> (f64, f64) {
    if mode == Mode::Ode {
        return stop;
    }
    let toks = crate::math_function::parse(emitted);
    if toks.is_empty() {
        return stop;
    }
    // 修正文本写在射手自己的坐标系里，`flip` 的射手要把 x 取反再代。
    let x = if flip { -stop.0 } else { stop.0 };
    let y = crate::math_function::eval(&toks, x, 0.0, 0.0);
    if y.is_finite() {
        (stop.0, y)
    } else {
        stop
    }
}

/// 一次开火的**全部几何**：点序列 + 停止点 / 致死 + 两串修正文本 + 爆炸点。
///
/// 驱动的报文只需要从这里取字段，**不需要**（也不允许）自己再算一遍。
#[derive(Debug, Clone, PartialEq)]
pub struct ShotGeometry {
    /// 点序列 / 停止点 / 阻挡 / 致死（[`resolve_points_owned`] 的产物）。
    pub outcome: ShotOutcome,
    /// 两串修正文本。
    pub corrected: CorrectedText,
    /// 爆炸点 = 挖洞点（[`explosion_point`]）。
    pub explosion: (f64, f64),
}

/// 内核里**唯一**的"一条函数原文 → 全部坐标"入口。
///
/// `mask` 只应包含障碍；`targets` 可以**包含射手本人**（`owner` 会在内核里被排除，
/// 只比 id，不比位置 —— 见 [`resolve_points_owned`] 的文档与 `docs/122` §4）。
#[allow(clippy::too_many_arguments)]
pub fn resolve_shot(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    mask: &[u8],
    targets: &[Target],
    owner: Option<i64>,
) -> ShotGeometry {
    let corrected = corrected_text(text, x0, y0, flip, mode);
    // `apply_correction = false`：修正已经以**文本**形式在里面了（3 位小数那一版）。
    let points = shot_points(&corrected.sampled, x0, y0, step, flip, mode, false);
    let outcome = resolve_points_owned(mask, targets, &points, owner);
    let explosion = explosion_point(&corrected.emitted, outcome.stop, flip, mode);
    ShotGeometry { outcome, corrected, explosion }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plane::{empty_mask, rasterize};

    #[test]
    fn obstacle_stops_the_shot() {
        let mut m = empty_mask();
        rasterize(&mut m, -5.0, 0.0, 1.0, 0x00);
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[]);
        assert!(r.blocked, "应被障碍挡住");
        assert!(r.stop.0 > -6.5 && r.stop.0 < -3.5, "停止点应在障碍附近: {:?}", r.stop);
    }

    #[test]
    fn soldier_dies_without_stopping_the_shot() {
        let m = empty_mask(); // 无障碍
        let t = Target { id: 7, x: 0.0, y: 0.0, radius: 0.5 };
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[t]);
        assert_eq!(r.killed, vec![7], "经过士兵应判定致死");
        assert!(!r.blocked, "士兵不应阻挡弹道");
        assert!(r.stop.0 < -5.0, "弹道应继续飞过士兵: {:?}", r.stop);
    }

    #[test]
    fn kill_only_counts_once() {
        let m = empty_mask();
        let t = Target { id: 9, x: 0.0, y: 0.0, radius: 2.0 };
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[t]);
        assert_eq!(r.killed, vec![9], "同一士兵只应记一次");
    }

    #[test]
    fn miss_when_far_away() {
        let m = empty_mask();
        let t = Target { id: 3, x: 0.0, y: 20.0, radius: 0.5 };
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[t]);
        assert!(r.killed.is_empty());
    }

    // -----------------------------------------------------------------------
    // 坐标计算只有一份（`docs/06-verification.md` §30）
    //
    // 下面两条**机械**地把新入口钉在 `game.rs` 那两份既有实现上：
    // 谁改动其中一边而另一边没跟上，这里立刻红。等 `game.rs` 下次被它的
    // 负责人动到时，应当改成直接调本节，那时这两条就成了恒等式（可以删）。
    // -----------------------------------------------------------------------

    /// 原文样本全部来自只读语料（名字即回放 id），覆盖四种括号形态。
    const CORPUS_TEXTS: &[(&str, f64, f64, bool)] = &[
        // 普通：0 常数、负常数
        ("0.00", 1.651220991201045, 4.638020825690482, false),
        ("x", -13.429861913415694, -3.686102115928783, false),
        // 缺 `)` 3 个（回放 1782642986）
        ("5/155sin(4exp(4*6cos(", 0.0, 6.055, false),
        // 多 1 个 `)`（回放 1782675382#0）
        (
            "14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2))",
            22.908325288230134,
            -4.32330978206142,
            true,
        ),
        // 空函数（真值只发后缀）
        ("", -13.429861913415694, -3.686102115928783, false),
        // 幂 / 三角函数混排
        ("sin(3x)+x^2/4", 5.5, -1.25, true),
    ];

    #[test]
    fn corrected_text_is_the_same_rule_as_the_kernel_copy() {
        for (text, x0, y0, flip) in CORPUS_TEXTS {
            let c = corrected_text(text, *x0, *y0, *flip, Mode::Normal);
            // `game.rs` 私有的两份实现（`pub(crate)`）：发出的串与采样的串。
            assert_eq!(c.emitted, crate::game::corrected_function_text(text, *x0, *y0, *flip));
            assert_eq!(c.sampled, crate::game::sampling_function_text(text, *x0, *y0, *flip));
        }
    }

    #[test]
    fn diffeq_corrected_text_has_no_suffix() {
        for (text, _x0, _y0, _flip) in CORPUS_TEXTS {
            let c = corrected_text(text, 3.0, 4.0, false, Mode::Ode);
            assert_eq!(c.emitted, *text, "DiffEq 局不加后缀（回放 550/550）");
            assert_eq!(c.sampled, *text);
        }
    }

    #[test]
    fn explosion_point_is_the_same_rule_as_the_kernel_copy() {
        // `game::hole_point(diff_eq, 原文, x0, y0, flip, stop)` 是既有实现。
        let stops = [(0.0, 0.0), (12.5, 3.25), (-4.5, -14.75)];
        for (text, x0, y0, flip) in CORPUS_TEXTS {
            // 空函数是**已知分歧**（见下一条测试），这里只比非空原文。
            if text.is_empty() {
                continue;
            }
            for stop in stops {
                let emitted = corrected_text(text, *x0, *y0, *flip, Mode::Normal).emitted;
                let p = explosion_point(&emitted, stop, *flip, Mode::Normal);
                let q = crate::game::hole_point(false, text, *x0, *y0, *flip, stop);
                assert_eq!(p.0.to_bits(), q.0.to_bits(), "{text} @ {stop:?}");
                assert_eq!(p.1.to_bits(), q.1.to_bits(), "{text} @ {stop:?}");
                // DiffEq 局：两边都不重算
                assert_eq!(
                    explosion_point(text, stop, *flip, Mode::Ode),
                    crate::game::hole_point(true, text, *x0, *y0, *flip, stop)
                );
            }
        }
    }

    /// **已知分歧**（不是 bug 掩盖，写成断言是为了它不许再偷偷变）：空函数。
    ///
    /// `game::hole_point` 在**原文**解析为空时直接返回停点；本节按"拿发出去的
    /// 那一串（`" - 3.686"`）在 `end_x` 处求值"处理，于是 y = 常数。
    /// 后者与"客户端看到的就是印出来的那行字"一致，也是本轮**沿用**的收敛层
    /// 现有行为（语料里 4 发）。要改的是 `game.rs` 那一侧（不在本任务范围）。
    #[test]
    fn empty_function_is_a_documented_divergence_with_game_rs() {
        let (x0, y0, flip) = (-13.429861913415694, -3.686102115928783, false);
        let stop = (2.0, 3.5);
        let emitted = corrected_text("", x0, y0, flip, Mode::Normal).emitted;
        assert_eq!(emitted, " - 3.686");
        // 本节：回代 = 常数
        assert_eq!(explosion_point(&emitted, stop, flip, Mode::Normal), (2.0, -3.686));
        // game.rs：原文为空 ⇒ 停点
        assert_eq!(crate::game::hole_point(false, "", x0, y0, flip, stop), stop);
    }

    /// 非有限值不是坐标：`1/(0)` 的回代是 `±inf`，必须回退到停点。
    #[test]
    fn explosion_point_falls_back_on_non_finite_values() {
        let stop = (2.0, 3.5);
        assert_eq!(explosion_point("1/(0)", stop, false, Mode::Normal), stop);
        assert_eq!(explosion_point("-1/(0)", stop, false, Mode::Normal), stop);
        // 负样本：有限值照常回代
        assert_eq!(explosion_point("0.00", stop, false, Mode::Normal), (2.0, 0.0));
    }

    /// [`resolve_shot`] 的几何与"手工拼积木"逐位相同（点序列 / stop / killed）。
    #[test]
    fn resolve_shot_geometry_is_the_composition_of_its_parts() {
        let mut m = empty_mask();
        // 障碍在 +x 一侧，弹道打完 7 号之后撞上它
        rasterize(&mut m, 5.0, 0.0, 1.0, 0x00);
        let targets = [
            Target { id: 9, x: 1.0, y: 0.0, radius: 0.5 },
            Target { id: 7, x: 3.0, y: 0.0, radius: 0.5 },
        ];
        let g = resolve_shot(
            "0.00",
            0.0,
            0.0,
            crate::SERVER_STEP,
            false,
            Mode::Normal,
            &m,
            &targets,
            Some(9),
        );
        let c = corrected_text("0.00", 0.0, 0.0, false, Mode::Normal);
        let pts = shot_points(&c.sampled, 0.0, 0.0, crate::SERVER_STEP, false, Mode::Normal, false);
        assert_eq!(g.outcome.points, pts, "点序列必须来自 shot_points，不许另算");
        assert_eq!(g.outcome.killed, vec![7], "射手 9 号被内核排除，7 号命中");
        assert!(g.outcome.blocked, "弹道应撞上 +x 的障碍");
        assert!(g.outcome.stop.0 > 3.5 && g.outcome.stop.0 < 5.0, "停点={:?}", g.outcome.stop);
        assert_eq!(g.explosion.0.to_bits(), g.outcome.stop.0.to_bits());
        assert_eq!(g.explosion.1.to_bits(), 0.0_f64.to_bits(), "常数函数 ⇒ 爆炸点 y = 0");
    }
}

// ---------------------------------------------------------------------------
// 事件生成：把结算结果变成协议消息
// ---------------------------------------------------------------------------

/// 一次开火会产生的协议事件（顺序即协议上的出现顺序）。
///
/// 依据真实抓包：
///
/// ```text
/// FunctionPoints -> LifeQueuedDeath -> EffectInfo(DeathEffect) -> ... -> EffectInfo(ExplosionEffect)
/// ```
///
/// 若弹道被障碍挡住，还会多一条 `QueueDestroyObstacle`（位置 = 停止点）。
/// **tick 值没有建模**：真实 tick 取决于弹丸飞行时间，尚未还原，
/// 所以这里不带 tick 字段，只保证种类与坐标一致。
#[derive(Debug, Clone, PartialEq)]
pub enum ShotEvent {
    FunctionPoints { owner_id: i64, start_x: f64, start_y: f64, end_x: f64 },
    LifeQueuedDeath { entity_id: i64 },
    DeathEffect { entity_id: i64, x: f64, y: f64 },
    QueueDestroyObstacle { x: f64, y: f64, radius: f64 },
    ExplosionEffect { x: f64, y: f64 },
}

/// 由结算结果生成事件序列。
pub fn events(owner_id: i64, x0: f64, y0: f64, outcome: &ShotOutcome, targets: &[Target]) -> Vec<ShotEvent> {
    let mut out = vec![ShotEvent::FunctionPoints {
        owner_id,
        start_x: x0,
        start_y: y0,
        end_x: outcome.stop.0,
    }];
    for id in &outcome.killed {
        out.push(ShotEvent::LifeQueuedDeath { entity_id: *id });
        if let Some(t) = targets.iter().find(|t| t.id == *id) {
            // 死亡效果点在**士兵站立处**，不是弹道上的点（docs/30）
            out.push(ShotEvent::DeathEffect { entity_id: *id, x: t.x, y: t.y });
        }
    }
    if outcome.blocked {
        out.push(ShotEvent::QueueDestroyObstacle { x: outcome.stop.0, y: outcome.stop.1, radius: 0.5 });
    }
    out.push(ShotEvent::ExplosionEffect { x: outcome.stop.0, y: outcome.stop.1 });
    out
}
