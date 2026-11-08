//! **假设空间整片扫描**（`dispatch/53-corpus-sweep.md`）：把 H1–H5 逐个换成开关，
//! 对整份语料重跑 `fire` 轴，看有没有任何一种解释能把内容命中数顶过基线。
//!
//! ```text
//! cargo run --release --offline --example hypothesis_sweep -- \
//!     --dir <快照> --shard 0/32 --configs all --json out/shard-00.jsonl
//!
//!   --shard i/N   只跑按文件名排序后 `序号 % N == i` 的那些回放（并行分片，互不重叠）
//!   --configs all|H0|H2a,...   要跑哪几档假设（默认 all）
//!   --verify      对 `H0` 额外做**正对照**：我方复刻的采样器与内核 `calculate` 逐位比
//!   --max-diffs   透传给 `compare_batch`（默认 16）
//! ```
//!
//! ## 为什么是"开关住在 examples/ 里"
//!
//! 任务书的硬约束：**不许改 `src/`**（这是"试试别的解释"，不是"改实现"）。所以这里
//! 一个 `src/` 的行都没动，用的是三个**本来就公开**的接口：
//!
//! * `Match::fire_functions(&shots) -> Vec<ShotOutcome>`（`&self`，不改状态）
//! * `Match::execute_results(&shots, &outcomes, now_tick) -> Vec<String>`
//!   —— 报文**从 outcome 里取字段**，所以换掉 outcome 就等于换了"解释"；
//! * `settle::resolve_points_owned(mask, targets, points, owner)` —— 点序列 → 停点/致死。
//!
//! `Match::fire_batch` = 上面两条连着调用（`src/game.rs`），所以本工具对 `H0`（基线档）
//! 必须**逐字节复现** `replay_attrib --drive fire` 的 647/1075；`--verify` 会逐批比对
//! 我方复刻的采样器与内核的 `calculate`。
//!
//! ## 假设怎么落成开关（每条都独立，单变量轮换）
//!
//! ```text
//! H1  end_x   = last / prev / first_oob / interp        （只动 FunctionPoints.end_x）
//! H2  pos     = last / prev / first_oob / interp        （只动 EffectInfo(Explosion) 的 pos_*）
//! H3  sampled 修正常量 = {:.3}（现行）/ {:.17}（全精度）/ 截断到 3 位
//! H4  采样终止 = 越界即停（现行）/ 越界后再走一步 / 停在最后一个 inside 点
//! H5  DiffEq 步长 = 0.01（现行）/ 0.005 / 自适应
//! ```
//!
//! H1/H2 只改**报文里的字段**（点序列一个比特都没动），H3/H4/H5 改**点序列本身**
//! （于是停点、致死、挖洞都跟着变 —— 那才是"另一种解释"的完整后果）。
//!
//! 证据等级：本工具产出的一切都是**我方实现的假设对比**，不是真值。判据只是
//! "内容命中数是否稳定超过基线"，超过的才有必要回二进制深挖。
use gw2_rebuild::_scaffold::replaydiff::{
    compare_batch, split_batches, Batch, Inputs, Kind, Msg, Policy, Replay, ShotInput,
};
use gw2_rebuild::game::Match;
use gw2_rebuild::graphwar_path_calculator::{point_budget, Mode};
use gw2_rebuild::graphwar_shot_calculator::settle::{
    explosion_point, resolve_points_owned, ShotOutcome, Target,
};
use gw2_rebuild::math_function::{eval, parse};
use gw2_rebuild::plane::{empty_mask, rasterize};
use gw2_rebuild::{Token, LENGTH_TOL, MAX_ABS_X, MAX_ABS_Y, SERVER_STEP};
use std::io::Write;

// ---------------------------------------------------------------------------
// 1. 假设的取值
// ---------------------------------------------------------------------------

/// "用哪一个点"（H1 / H2 共用同一套四种取法）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sel {
    /// `pts[len-1]`（现行）
    Last,
    /// `pts[len-2]`
    Prev,
    /// 首个越界点（采样器因越界而丢掉的那一步）
    FirstOob,
    /// 最后一个 inside 点与首个越界点之间**插值到边界**
    Interp,
}

impl Sel {
    fn name(self) -> &'static str {
        match self {
            Sel::Last => "last",
            Sel::Prev => "prev",
            Sel::FirstOob => "first_oob",
            Sel::Interp => "interp",
        }
    }
}

/// 修正常数印成几位（H3）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CPrec {
    /// 现行：`{:.3}`（`docs/06` §25：采样器吃的就是印出来的三位小数）
    Three,
    /// 全精度：`{:.17}`（定点，任何值都能被 `lex` 吃回去）
    Full,
    /// 向零截断到 3 位再印（不是四舍五入）
    Trunc,
}

impl CPrec {
    fn name(self) -> &'static str {
        match self {
            CPrec::Three => "3dp",
            CPrec::Full => "full",
            CPrec::Trunc => "trunc3",
        }
    }
}

/// 采样终止条件（H4）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Term {
    /// 现行：越界就整条曲线结束，越界点**不入列**（`graphwar_path_calculator` 的两条 return）
    StopAtOob,
    /// 越界后再走一步：把那个越界点也入列，然后结束
    OneMore,
    /// 停在最后一个 inside 点 —— 注意：**现行实现已经如此**（入列前就查越界），
    /// 所以这一档与 `StopAtOob` 在结构上等价；跑它就是为了把这一点证出来。
    LastInside,
}

impl Term {
    fn name(self) -> &'static str {
        match self {
            Term::StopAtOob => "stop",
            Term::OneMore => "one_more",
            Term::LastInside => "last_inside",
        }
    }
}

/// 采样步长（H5）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StepSel {
    /// 现行：`SERVER_STEP` = 0.01（两轴都用）
    Server,
    /// `DiffEq` 局用 0.005（RK4 积分步长减半），`Normal` 局不动
    Half,
    /// 两轴都减半（对照档：证明"只动 DiffEq"不是被 Normal 的步长带偏的）
    HalfAll,
    /// 一条**具体的**自适应规则（见 [`step_for`]）；只作用于 `DiffEq` 局
    Adaptive,
}

impl StepSel {
    fn name(self) -> &'static str {
        match self {
            StepSel::Server => "0.01",
            StepSel::Half => "0.005",
            StepSel::HalfAll => "0.005_all",
            StepSel::Adaptive => "adaptive",
        }
    }
}

/// 一档完整的假设。
#[derive(Clone, Copy, PartialEq, Debug)]
struct Hyp {
    end_x: Sel,
    pos: Sel,
    cprec: CPrec,
    term: Term,
    step: StepSel,
}

impl Hyp {
    /// 基线 = 现行实现（`replay_attrib --drive fire` 的那条路）。
    const BASE: Hyp = Hyp {
        end_x: Sel::Last,
        pos: Sel::Last,
        cprec: CPrec::Three,
        term: Term::StopAtOob,
        step: StepSel::Server,
    };

    fn label(&self) -> String {
        format!(
            "end_x={} pos={} c={} term={} step={}",
            self.end_x.name(),
            self.pos.name(),
            self.cprec.name(),
            self.term.name(),
            self.step.name()
        )
    }
}

/// 一档命名的假设（名字进汇总表）。
struct Named {
    id: String,
    hyp: Hyp,
}

/// 要跑的假设清单：**先单变量轮换**，再补 H1×H2 的 4×4 全组合。
///
/// 为什么把 16 个组合一并跑：H1 与 H2 在实现里是**同一次读**（都取 `out.stop`），
/// 只跑单变量的话"两者都换"这一格仍然空着；组合总数 4×4=16 而已，跑完这张表
/// 才真的把这一段假设空间关掉。
fn all_configs() -> Vec<Named> {
    let mut v: Vec<Named> = Vec::new();
    let mut push = |id: &str, hyp: Hyp| v.push(Named { id: id.to_string(), hyp });

    push("H0", Hyp::BASE);

    // ---- H1：只换 end_x ----
    for (id, sel) in [
        ("H1a", Sel::Prev),
        ("H1b", Sel::FirstOob),
        ("H1c", Sel::Interp),
    ] {
        push(id, Hyp { end_x: sel, ..Hyp::BASE });
    }
    // ---- H2：只换 pos ----
    for (id, sel) in [
        ("H2a", Sel::Prev),
        ("H2b", Sel::FirstOob),
        ("H2c", Sel::Interp),
    ] {
        push(id, Hyp { pos: sel, ..Hyp::BASE });
    }
    // ---- H3：修正常数的精度 ----
    push("H3a", Hyp { cprec: CPrec::Full, ..Hyp::BASE });
    push("H3b", Hyp { cprec: CPrec::Trunc, ..Hyp::BASE });
    // ---- H4：采样终止 ----
    push("H4a", Hyp { term: Term::OneMore, ..Hyp::BASE });
    push("H4b", Hyp { term: Term::LastInside, ..Hyp::BASE });
    // ---- H5：DiffEq 积分步长 ----
    push("H5a", Hyp { step: StepSel::Half, ..Hyp::BASE });
    push("H5b", Hyp { step: StepSel::Adaptive, ..Hyp::BASE });
    // H5c：步长减半**两轴都吃**（对照：H5a 只动 DiffEq 局）
    push("H5c", Hyp { step: StepSel::HalfAll, ..Hyp::BASE });

    // ---- H1×H2 的 4×4 全组合（`G<end_x>_<pos>`，去掉与 H0 重合的 last/last）----
    let sels = [Sel::Last, Sel::Prev, Sel::FirstOob, Sel::Interp];
    for &e in &sels {
        for &p in &sels {
            if e == Sel::Last && p == Sel::Last {
                continue;
            }
            push(
                &format!("G{}_{}", e.name(), p.name()),
                Hyp { end_x: e, pos: p, ..Hyp::BASE },
            );
        }
    }
    v
}

// ---------------------------------------------------------------------------
// 2. 采样器：`src/graphwar_path_calculator.rs::calculate` 的逐行复刻 + 终止开关
// ---------------------------------------------------------------------------

/// 弦长。内核用的是 `sqrt(a*a + b*b)`（`math_function::m_hypot`，`pub(crate)`），
/// 不是真正的 `hypot` —— 这里照抄，别换成 `f64::hypot`（后者抗溢出，数值不同）。
#[inline]
fn chord(a: f64, b: f64) -> f64 {
    (a * a + b * b).sqrt()
}

/// 一次采样的结果：点序列 + **首个越界点**（现行实现会把它丢掉，假设里要用它）。
struct Sampled {
    points: Vec<(f64, f64)>,
    /// 采样器因"越界"而提前结束的那一步；因点数预算 / 不收敛结束时为 `None`。
    first_oob: Option<(f64, f64)>,
}

/// `graphwar_path_calculator::calculate` 的复刻（同一份算术、同一份早退），
/// 唯一的新增是 `term` 开关与把"被丢掉的那一步"交出来。
///
/// 锚点（对不上就说明复刻漂了）：
/// * `HALF = 0.5`、`SIXTH = 6.0`、`MAX_BISECT = 100`；
/// * 收敛判据 `!(LENGTH_TOL > (chord - step).abs())`，NaN 走 `hi = mid`；
/// * `Normal` 分支的半步复检 `chord > chord2` 不成立时 **lo 与 hi 都改**；
/// * 入列前 `cand_y.is_nan() || px.is_nan()` ⇒ 结束；`|mid| > MAX_ABS_X || |cand_y| > MAX_ABS_Y` ⇒ 结束。
fn sample(
    tokens: &[Token],
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    term: Term,
) -> Sampled {
    let mut pts = vec![(x0, y0)];
    let mut x = if flip { -x0 } else { x0 };
    let mut y = y0;
    let n = point_budget(step);
    let mut count: i32 = 0;

    loop {
        let mut lo = x;
        let mut hi = x + step;
        let mut accepted = false;

        for _ in 0..100usize {
            let mid = (lo + hi) * 0.5;

            let (cand_y, chord_len) = match mode {
                Mode::Ode => {
                    let h = mid - x;
                    let k1 = eval(tokens, x, y, 0.0);
                    let k2 = eval(tokens, x + h * 0.5, y + k1 * h * 0.5, 0.0);
                    let k3 = eval(tokens, x + h * 0.5, y + k2 * h * 0.5, 0.0);
                    let k4 = eval(tokens, x + h, y + k3 * h, 0.0);
                    let yn = y + (h / 6.0) * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
                    (yn, chord(h, yn - y))
                }
                Mode::Normal => {
                    let fm = eval(tokens, mid, 0.0, 0.0);
                    (fm, chord(mid - x, fm - y))
                }
            };

            if !(LENGTH_TOL > (chord_len - step).abs()) {
                if chord_len > step || chord_len.is_nan() {
                    hi = mid;
                } else {
                    lo = mid;
                }
                continue;
            }

            if mode == Mode::Normal {
                let mid2 = (x + mid) * 0.5;
                let fm2 = eval(tokens, mid2, 0.0, 0.0);
                let chord2 = chord(mid2 - x, fm2 - y);
                if !(chord_len > chord2) {
                    lo = x;
                    hi = mid2;
                    continue;
                }
            }

            let px = if flip { -mid } else { mid };
            if cand_y.is_nan() || px.is_nan() {
                return Sampled { points: pts, first_oob: None };
            }
            if mid.abs() > MAX_ABS_X || cand_y.abs() > MAX_ABS_Y {
                let oob = (px, cand_y);
                if term == Term::OneMore {
                    // "越界后再走一步"：那一步**入列**，然后整条曲线结束。
                    pts.push(oob);
                }
                return Sampled { points: pts, first_oob: Some(oob) };
            }

            pts.push((px, cand_y));
            count += 1;
            x = mid;
            y = cand_y;
            accepted = true;
            break;
        }

        if !accepted || count == n {
            return Sampled { points: pts, first_oob: None };
        }
    }
}

// ---------------------------------------------------------------------------
// 3. 修正文本（H3 的开关在这里）：两串规则与 `settle::corrected_text` 同源
// ---------------------------------------------------------------------------

/// `settle::corrected_text` 里那个 `balanced_texts` 的复刻（`pub(crate)`，例子里调不到）。
/// 返回 `(只补括号, 去掉多余 `)` 再补齐)`。
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

/// 带符号的修正常数后缀。`Cprec::Three` 必须与 `correction_text` **逐字节**相同
/// （`--verify` 会核对）；`NaN` 走 `+` 那一侧（原程序的 `c < 0.0` 分支）。
fn suffix(c: f64, prec: CPrec) -> String {
    if c.is_nan() {
        return " + NaN".to_string();
    }
    let mag = match prec {
        CPrec::Three => format!("{:.3}", c.abs()),
        CPrec::Full => format!("{:.17}", c.abs()),
        CPrec::Trunc => format!("{:.3}", (c.abs() * 1000.0).trunc() / 1000.0),
    };
    if c >= 0.0 {
        format!(" + {mag}")
    } else {
        format!(" - {mag}")
    }
}

/// 采样器吃的那一串（`Mode::Ode` 局 = 原文，不加常数、也不补括号 —— `docs/06` §29）。
fn sampled_text(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode, prec: CPrec) -> String {
    if mode == Mode::Ode {
        return text.to_string();
    }
    let (_, samplable) = balanced_texts(text);
    let toks = parse(&samplable);
    let c = y0
        - eval(
            &toks,
            if flip { -x0 } else { x0 },
            0.0,
            0.0,
        );
    format!("{samplable}{}", suffix(c, prec))
}

/// 报文里发出去的那一串（H1/H2/H3 都不改它：它就是回放 739/739 逐字节核对过的那串）。
fn emitted_text(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode) -> String {
    if mode == Mode::Ode {
        return text.to_string();
    }
    let (closed, samplable) = balanced_texts(text);
    let toks = parse(&samplable);
    let c = y0 - eval(&toks, if flip { -x0 } else { x0 }, 0.0, 0.0);
    format!("{closed}{}", suffix(c, CPrec::Three))
}

/// 本发用哪个步长（H5）。
///
/// `Adaptive` 是一条**具体的**规则：`h = 0.01 / (1 + |f(x0,y0)|)`，夹在 `[1e-3, 1e-2]`。
/// ⚠ 它只是"自适应"这一类里**一个**可复跑的实例，不是自适应方案的全集 ——
/// 所以 H5b 的数字只能否掉这条规则，不能否掉"所有自适应"。
fn step_for(step: StepSel, tokens: &[Token], x0: f64, y0: f64, mode: Mode) -> f64 {
    if mode != Mode::Ode {
        // `Normal` 的步长是"弦长目标"，不是积分步长：动它就不是 H5 了
        // （`HalfAll` 是刻意的对照档，用来证明 H5a 的效果不是被 Normal 那半边带偏的）
        return match step {
            StepSel::HalfAll => SERVER_STEP * 0.5,
            _ => SERVER_STEP,
        };
    }
    match step {
        StepSel::Server => SERVER_STEP,
        StepSel::Half | StepSel::HalfAll => SERVER_STEP * 0.5,
        StepSel::Adaptive => {
            let dydx = eval(tokens, x0, y0, 0.0).abs();
            let h = SERVER_STEP / (1.0 + dydx);
            if h.is_finite() {
                h.clamp(1e-3, SERVER_STEP)
            } else {
                SERVER_STEP
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 4. "用哪一个点"（H1 / H2）
// ---------------------------------------------------------------------------

/// 线段 `p → q` 与棋盘边界（`|x| = MAX_ABS_X` / `|y| = MAX_ABS_Y`）的交点。
/// 两轴都越界时取**先**穿过的那一个。
fn interp_to_boundary(p: (f64, f64), q: (f64, f64)) -> (f64, f64) {
    let mut t = 1.0f64;
    if q.0.abs() > MAX_ABS_X && q.0 != p.0 {
        let bx = MAX_ABS_X * q.0.signum();
        let ti = (bx - p.0) / (q.0 - p.0);
        if ti.is_finite() {
            t = t.min(ti.clamp(0.0, 1.0));
        }
    }
    if q.1.abs() > MAX_ABS_Y && q.1 != p.1 {
        let by = MAX_ABS_Y * q.1.signum();
        let ti = (by - p.1) / (q.1 - p.1);
        if ti.is_finite() {
            t = t.min(ti.clamp(0.0, 1.0));
        }
    }
    (p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1))
}

/// 一次采样的"停在哪儿"：真值里的 `pts` 是**被障碍截断之后**那块缓冲（`docs/06` §29.4：
/// 5098 → 1528、`pos = pts[1527]`），所以 `pts[len-1]` 指的是**停点**那一格，
/// 不是整条采样的最后一格。这一层把三种停法统一成"数组 + 停点下标"。
struct StopSel {
    /// 采样器给出的整条点序列（未截断）
    pts: Vec<(f64, f64)>,
    /// 采样器因越界而丢掉的那一步
    oob: Option<(f64, f64)>,
    /// 停点在 `pts` 里的下标（被障碍挡住 = 挡住那一格；否则 = 最后一格）
    idx: usize,
    /// 是否被障碍挡住（挡住时"停点之后的下一步"要取 `pts[idx+1]`，越界那一步对它没意义）
    blocked: bool,
}

impl StopSel {
    fn new(outcome: &ShotOutcome, oob: Option<(f64, f64)>) -> StopSel {
        let pts = outcome.points.clone();
        let idx = if pts.is_empty() {
            0
        } else if !outcome.blocked {
            pts.len() - 1
        } else {
            pts.iter()
                .position(|p| p.0 == outcome.stop.0 && p.1 == outcome.stop.1)
                .unwrap_or(pts.len() - 1)
        };
        StopSel { pts, oob, idx, blocked: outcome.blocked }
    }

    /// 停点（基线取法）。
    fn stop(&self) -> (f64, f64) {
        match self.pts.get(self.idx) {
            Some(p) => *p,
            None => (f64::NAN, f64::NAN),
        }
    }

    /// 停点**之后**的下一步：被障碍挡住 ⇒ `pts[idx+1]`；否则 ⇒ 采样器丢掉的越界步。
    fn beyond(&self) -> Option<(f64, f64)> {
        if self.blocked {
            self.pts.get(self.idx + 1).copied()
        } else {
            self.oob
        }
    }
}

/// 按 `sel` 从"数组 + 停点"里取点；取不到就退回停点（⇒ 该档在这一发上是**无操作**）。
fn pick(s: &StopSel, sel: Sel) -> (f64, f64) {
    let last = s.stop();
    match sel {
        Sel::Last => last,
        Sel::Prev => {
            if s.idx >= 1 {
                s.pts[s.idx - 1]
            } else {
                last
            }
        }
        Sel::FirstOob => s.beyond().unwrap_or(last),
        Sel::Interp => match s.beyond() {
            Some(q) => interp_to_boundary(last, q),
            None => last,
        },
    }
}

// ---------------------------------------------------------------------------
// 5. 报文补丁（H1 / H2 只动字段，不动点序列）
// ---------------------------------------------------------------------------

/// `{"Kind":{...}}` 里某个数值字段改值 —— 必须连 `raw` 一起改：`compare_msg` 先比
/// `raw` 文本，只改 `kind` 会在 `NumericFormat` 那档上假通过。
fn set_num(m: &mut Msg, name: &str, v: f64) {
    if let Some(f) = m.body.iter_mut().find(|f| f.name == name) {
        f.val.raw = gw2_rebuild::_scaffold::protocol::emit_f64(v);
        f.val.kind = Kind::Num(v);
    }
}

/// 报文里 `EffectInfo` 的 `effect_type`。
fn effect_type(m: &Msg) -> Option<String> {
    if m.kind == "EffectInfo" {
        m.get_str("effect_type")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 6. 一档假设在**一份回放**上的完整跑法
// ---------------------------------------------------------------------------

/// 一份回放 + 一档假设的结果。
#[derive(Default, Clone)]
struct Tally {
    batches: usize,
    content_ok: usize,
    /// `H0` 的正对照：我方复刻采样器与内核 `calculate` 点序列**不逐位相等**的发数
    parity_bad: usize,
    parity_shots: usize,
    /// `H0` 的正对照（之二）：我方复刻的**发出去的修正文本**与报文里那串不逐字节相等的发数
    emitted_bad: usize,
    /// 我方 emitter 产出非法 JSON 的报文条数（与 `replay_attrib` 同口径）
    invalid: usize,
    /// 逐批明细（`--batches` 才写出去；只用于翻转集分析，不入库）
    per_batch: Vec<BatchRec>,
}

/// 一批的诊断记录。
#[derive(Clone, Copy)]
struct BatchRec {
    idx: usize,
    shots: usize,
    /// 这一批里最长的那条弹道有几个点（1 = 采样器第一步就没走出去）
    max_points: usize,
    /// 是否所有发都因"点数预算 / 不收敛"结束（⇒ 没有越界点，`first_oob` 档退化成 `last`）
    no_oob: bool,
    /// 这一档在这一批上**什么都没改**（四种取法都退回停点）
    noop: bool,
    ok: bool,
}

/// 跑一份回放的一档假设。
/// 一档假设**是否改点序列本身**。
///
/// `H1`/`H2`/H4b 都是"点序列一个比特都不动、只换读哪一点 / 终止档与现行等价"，所以
/// 它们共用**同一次**状态演化（`execute_results` 的致死 / 挖洞 / effect_id 全一样）：
/// 一次跑完，再把报文**克隆**出来逐档打补丁。这一条把 29 趟压到 7 趟，且不损失任何东西
/// —— 补丁只改报文里的字段，不进任何状态。
fn field_only(h: &Hyp) -> bool {
    h.cprec == CPrec::Three
        && h.step == StepSel::Server
        && matches!(h.term, Term::StopAtOob | Term::LastInside)
}

/// 跑**一趟**：`cfgs` 里第一个的工作点序列决定这一趟的采样，其余各档只在报文上打补丁。
///
/// * `cfgs.len() == 1` ⇒ 单档整趟（`H3`/`H4a`/`H5*` 这种改点序列的档走这条）；
/// * `cfgs.len() > 1` ⇒ 共享趟，要求**每一档的采样设置相同**（调用方用 [`field_only`] 保证）。
///
/// `verify` 只在 `H0` 上做两件正对照：点序列与内核 `calculate` 逐位相同、发出去的修正文本
/// 与内核逐字节相同。
fn run_pass(replay: &Replay, batches: &[Batch], cfgs: &[Named], verify: bool) -> Vec<Tally> {
    let policy = Policy::default();
    let mut ts: Vec<Tally> = vec![Tally::default(); cfgs.len()];
    let Some(first) = batches.first() else { return ts };
    let base_hyp = cfgs[0].hyp;
    if cfgs.len() > 1 {
        for c in cfgs {
            assert!(
                c.hyp.cprec == base_hyp.cprec
                    && c.hyp.step == base_hyp.step
                    && sample_term_eq(c.hyp.term, base_hyp.term),
                "共享趟里的各档必须采样设置相同：{} vs {}",
                c.id,
                cfgs[0].id
            );
        }
    }
    let inputs = Inputs::scan(replay, first.start);
    let soldiers: Vec<Target> = inputs
        .pos
        .iter()
        .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
        .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();
    if soldiers.is_empty() {
        return ts;
    }
    let base_tick = replay.msgs[first.start].tick;
    let mut model = Match::new(
        inputs.room.clone(),
        inputs.obstacles.clone(),
        soldiers,
        (base_tick, 0),
    );
    for id in inputs.targets.iter() {
        model.add_target(*id, 1);
    }
    for (id, g) in inputs.skins.iter() {
        let _ = model.note_skin_graphics(*id, g);
    }
    // 我方那份掩膜：与 `Match::new` 的建法逐行相同（`empty_mask` + 每障碍一次 `rasterize`）。
    // `Match::mask` 是私有的，所以这里自带一份；挖洞时与 `execute_results` 同步光栅化。
    let mut mask = empty_mask();
    for (x, y, r) in &model.obstacles {
        rasterize(&mut mask, *x, *y, *r, 0x00);
    }

    // `FunctionModeInfo` 时刻表（与 `Predictor::sync_mode` 同一条规则）
    let mut mode_schedule: Vec<(usize, String)> = Vec::new();
    for (i, m) in replay.msgs.iter().enumerate() {
        if m.kind == "FunctionModeInfo" {
            if let Some(s) = m.get_str("function_mode") {
                if mode_schedule.last().map(|e: &(usize, String)| e.1.as_str()) != Some(s.as_str()) {
                    mode_schedule.push((i, s));
                }
            }
        }
    }
    let mut mode_cursor = 0usize;

    for b in batches {
        while let Some((idx, fm)) = mode_schedule.get(mode_cursor) {
            if *idx >= b.start {
                break;
            }
            model.set_function_mode_wire(fm);
            mode_cursor += 1;
        }
        if b.shots.is_empty() {
            continue;
        }
        let mode = model.sampling_mode();
        let tick = replay.msgs[b.start].tick;
        model.tick_base = (tick, 0);
        let now_tick = model.tick_now(0);

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

        // ---- 每一发：算点序列（按本趟的采样设置）----
        let mut sampled: Vec<Sampled> = Vec::with_capacity(b.shots.len());
        for s in &b.shots {
            let text = sampled_text(&s.text, s.x0, s.y0, s.flip, mode, base_hyp.cprec);
            let toks = parse(&text);
            let step = step_for(base_hyp.step, &toks, s.x0, s.y0, mode);
            sampled.push(sample(&toks, s.x0, s.y0, step, s.flip, mode, base_hyp.term));
        }

        // ---- 正对照之一：内核走一遍 `fire_functions`，逐位比点序列 ----
        if verify && cfgs.iter().any(|c| c.id == "H0") {
            let native = model.fire_functions(&subs);
            for (i, n) in native.iter().enumerate() {
                ts[0].parity_shots += 1;
                if sampled[i].points != n.points {
                    ts[0].parity_bad += 1;
                }
            }
        }

        // ---- outcome：点序列 → 停点 / 致死 ----
        let targets = model.soldiers.clone();
        let outcomes: Vec<ShotOutcome> = b
            .shots
            .iter()
            .enumerate()
            .map(|(i, s)| resolve_points_owned(&mask, &targets, &sampled[i].points, Some(s.owner)))
            .collect();

        // 停点视角（`pts[len-1]` 到底指哪一格：被障碍截断时指**挡住那一格**）
        let stops: Vec<StopSel> = (0..b.shots.len())
            .map(|i| StopSel::new(&outcomes[i], sampled[i].first_oob))
            .collect();

        // ---- 报文（状态落地也在里面：致死 / 挖洞 / 计数）----
        let wire = model.execute_results(&subs, &outcomes, now_tick);

        // ---- 挖洞同步到我方掩膜（`execute_results` 里按 hole 点光栅化）----
        let mut base_msgs: Vec<Msg> = Vec::with_capacity(wire.len());
        let mut invalid = 0usize;
        for w in &wire {
            match Msg::parse_wire(w) {
                Ok(m) => base_msgs.push(m),
                Err(_) => invalid += 1,
            }
        }
        for m in &base_msgs {
            if m.kind == "QueueDestroyObstacle" {
                if let (Some(hx), Some(hy)) = (m.get_f64("pos_x"), m.get_f64("pos_y")) {
                    rasterize(&mut mask, hx, hy, 0.5, 0xFF);
                }
            }
        }

        // ---- 正对照之二：发出去的修正文本必须与内核逐字节相同 ----
        if verify && cfgs.iter().any(|c| c.id == "H0") {
            let mut k = 0usize;
            for m in base_msgs.iter() {
                if m.kind == "FunctionInfo" {
                    if let Some(s) = b.shots.get(k) {
                        let mine = emitted_text(&s.text, s.x0, s.y0, s.flip, mode);
                        if m.get_str("corrected_function").as_deref() != Some(mine.as_str()) {
                            ts[0].emitted_bad += 1;
                        }
                    }
                    k += 1;
                }
            }
        }

        let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();
        let max_points = sampled.iter().map(|s| s.points.len()).max().unwrap_or(0);
        let no_oob = sampled.iter().all(|s| s.first_oob.is_none());

        for (ci, cfg) in cfgs.iter().enumerate() {
            let mut ours = base_msgs.clone();
            // 这一档在这一批上是不是**无操作**（四种取法都退回停点 ⇒ 报文逐字节不变）
            // 位相等（`==` 对 NaN 永远 false，而空点序列的停点就是 NaN）
            let same = |a: (f64, f64), b: (f64, f64)| {
                a.0.to_bits() == b.0.to_bits() && a.1.to_bits() == b.1.to_bits()
            };
            let noop = stops.iter().all(|st| {
                let base = st.stop();
                same(pick(st, cfg.hyp.end_x), base) && same(pick(st, cfg.hyp.pos), base)
            });
            if cfg.hyp.end_x != Sel::Last || cfg.hyp.pos != Sel::Last {
                patch_fields(
                    &mut ours,
                    &b.shots,
                    &stops,
                    &outcomes,
                    mode,
                    cfg.hyp.end_x,
                    cfg.hyp.pos,
                );
            }
            let v = compare_batch(&ours, &observed, &policy, 16);
            let t = &mut ts[ci];
            t.invalid += invalid;
            t.batches += 1;
            if v.content_ok {
                t.content_ok += 1;
            }
            t.per_batch.push(BatchRec {
                idx: t.batches - 1,
                shots: b.shots.len(),
                max_points,
                no_oob,
                noop,
                ok: v.content_ok,
            });
        }
    }
    ts
}

/// `StopAtOob` 与 `LastInside` 在实现里是**同一个行为**（入列前就查越界 ⇒ 数组里的点全是
/// inside 点），所以它们可以同趟跑；`OneMore` 是另一趟。
fn sample_term_eq(a: Term, b: Term) -> bool {
    fn kind(t: Term) -> u8 {
        match t {
            Term::OneMore => 1,
            _ => 0,
        }
    }
    kind(a) == kind(b)
}

/// 把 `end_x` / `pos` 换成假设档选中的那个点（**只动这两个字段**）。
///
/// 报文与发的顺序（`docs/141` §2–§4）：
/// `FunctionPoints × N` 按发序、`QueueDestroyObstacle × B` 按发序（只对有阻挡的发）、
/// `FunctionActive + EffectInfo(Explosion)` 每发一对按发序。所以按下标对得上。
fn patch_fields(
    ours: &mut [Msg],
    shots: &[ShotInput],
    stops: &[StopSel],
    outcomes: &[ShotOutcome],
    mode: Mode,
    end_sel: Sel,
    pos_sel: Sel,
) {
    // 每条 `FunctionInfo` 里 `corrected_function` = 挖洞点要用的那一串（`hole_point` 的同源输入）
    let emitted: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        for m in ours.iter() {
            if m.kind == "FunctionInfo" {
                v.push(m.get_str("corrected_function").unwrap_or_default());
            }
        }
        v
    };

    // ① `FunctionPoints.end_x`
    if end_sel != Sel::Last {
        let mut k = 0usize;
        for m in ours.iter_mut() {
            if m.kind == "FunctionPoints" {
                if k < stops.len() {
                    let e = pick(&stops[k], end_sel);
                    set_num(m, "end_x", e.0);
                }
                k += 1;
            }
        }
    }

    // ② `EffectInfo(ExplosionEffect)` 的 pos_* —— 与 ③ 的挖洞点用同一个点
    if pos_sel != Sel::Last {
        let pos: Vec<(f64, f64)> = stops.iter().map(|s| pick(s, pos_sel)).collect();

        let mut k = 0usize;
        for m in ours.iter_mut() {
            if effect_type(m).as_deref() == Some("ExplosionEffect") {
                if k < pos.len() {
                    set_num(m, "pos_x", pos[k].0);
                    set_num(m, "pos_y", pos[k].1);
                }
                k += 1;
            }
        }

        // ③ `QueueDestroyObstacle`：真值里挖洞点 = 爆炸点（`docs/06` §24：503/503 逐位），
        //    所以挪了 pos 就必须把挖洞点一起挪过去，否则这条报文自己跟自己矛盾。
        let holes: Vec<Option<(f64, f64)>> = (0..shots.len())
            .map(|i| {
                if !outcomes[i].blocked {
                    return None;
                }
                let text = emitted.get(i).cloned().unwrap_or_default();
                if mode == Mode::Ode {
                    Some(pos[i])
                } else {
                    Some(explosion_point(&text, pos[i], shots[i].flip, Mode::Normal))
                }
            })
            .collect();
        let mut k = 0usize;
        for m in ours.iter_mut() {
            if m.kind == "QueueDestroyObstacle" {
                if let Some(Some(h)) = holes.get(k) {
                    set_num(m, "pos_x", h.0);
                    set_num(m, "pos_y", h.1);
                }
                k += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 7. main
// ---------------------------------------------------------------------------

fn jstr(s: &str) -> String {
    gw2_rebuild::_scaffold::replaydiff::wire::quote(s)
}

fn main() {
    let mut dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    let mut shard = (0usize, 1usize);
    let mut files_from: Option<std::path::PathBuf> = None;
    let mut cost_probe = false;
    let mut want: Option<Vec<String>> = None;
    let mut verify = false;
    let mut json_out: Option<std::path::PathBuf> = None;
    let mut batches_out: Option<std::path::PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--shard" => {
                let v = args.next().expect("--shard 需要 i/N");
                let (i, n) = v.split_once('/').expect("--shard 形如 0/32");
                shard = (i.parse().unwrap(), n.parse().unwrap());
                assert!(shard.0 < shard.1 && shard.1 > 0, "--shard 越界");
            }
            "--configs" => {
                let v = args.next().expect("--configs 需要值");
                if v != "all" {
                    want = Some(v.split(',').map(|s| s.trim().to_string()).collect());
                }
            }
            "--files" => files_from = Some(args.next().expect("--files 需要值").into()),
            "--cost-probe" => cost_probe = true,
            "--verify" => verify = true,
            "--json" => json_out = Some(args.next().expect("--json 需要值").into()),
            "--batches" => batches_out = Some(args.next().expect("--batches 需要值").into()),
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(2);
            }
        }
    }
    if !dir.is_dir() {
        eprintln!("回放目录不存在: {}", dir.display());
        std::process::exit(2);
    }

    let configs: Vec<Named> = {
        let all = all_configs();
        match &want {
            None => all,
            Some(ids) => {
                let mut out = Vec::new();
                for id in ids {
                    match all.iter().find(|c| &c.id == id) {
                        Some(c) => out.push(Named { id: c.id.clone(), hyp: c.hyp }),
                        None => {
                            eprintln!("未知假设档 `{id}`");
                            std::process::exit(2);
                        }
                    }
                }
                out
            }
        }
    };

    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    names.sort();
    // 分片：按排序后的序号取模（互不重叠、各片大小相差 ≤ 1）。
    // `--files` 给定时改用**显式清单**（并行驱动按 LPT 装桶，见
    // `tools/hypothesis_sweep_par.py`）—— 取模分片的毛病是最慢那一片决定整体墙钟。
    names = match &files_from {
        Some(list) => {
            let text = std::fs::read_to_string(list).expect("--files 读不了");
            text.lines()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| dir.join(s))
                .filter(|p| p.is_file())
                .collect()
        }
        None => names
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % shard.1 == shard.0)
            .map(|(_, p)| p)
            .collect(),
    };

    // ---- 成本探针：只解析、不预测；给并行驱动做装桶用的代价估计 ----
    if cost_probe {
        let mut out = std::io::BufWriter::new(std::io::stdout());
        for path in &names {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let replay = match Replay::load(path) {
                Ok(r) => r,
                Err(_) => {
                    writeln!(out, "{name}\t0\t0\t0").unwrap();
                    continue;
                }
            };
            let batches = split_batches(&replay);
            let mut n_shots = 0usize;
            let mut n_ode = 0usize;
            for b in &batches {
                if b.shots.is_empty() {
                    continue;
                }
                n_shots += b.shots.len();
                // 模式在**开火那一刻**读（`docs/06` §33）：只看批之前最后一个 `FunctionModeInfo`
                let mut ode = false;
                for m in &replay.msgs[..b.start] {
                    if m.kind == "FunctionModeInfo" {
                        ode = m.get_str("function_mode").as_deref() == Some("DiffEqFunction");
                    }
                }
                if ode {
                    n_ode += b.shots.len();
                }
            }
            writeln!(out, "{name}\t{}\t{n_shots}\t{n_ode}", batches.len()).unwrap();
        }
        out.flush().unwrap();
        return;
    }

    let mut sink: Box<dyn Write> = match &json_out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            Box::new(std::io::BufWriter::new(
                std::fs::File::create(p).expect("--json 打不开"),
            ))
        }
        None => Box::new(std::io::stdout()),
    };

    let mut bsink: Option<Box<dyn Write>> = batches_out.map(|p| {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Box::new(std::io::BufWriter::new(
            std::fs::File::create(p).expect("--batches 打不开"),
        )) as Box<dyn Write>
    });

    for path in &names {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let replay = match Replay::load(path) {
            Ok(r) => r,
            Err(e) => {
                writeln!(
                    sink,
                    "{{\"file\":{},\"config\":\"*\",\"error\":{}}}",
                    jstr(&name),
                    jstr(&e)
                )
                .unwrap();
                continue;
            }
        };
        let batches = split_batches(&replay);
        if batches.is_empty() {
            writeln!(
                sink,
                "{{\"file\":{},\"config\":\"*\",\"error\":\"没有结算批\"}}",
                jstr(&name)
            )
            .unwrap();
            continue;
        }
        // 共享趟一次跑掉所有"只换字段"的档；改点序列的档各跑一趟。
        let mut groups: Vec<Vec<Named>> = Vec::new();
        let shared: Vec<Named> = configs
            .iter()
            .filter(|c| field_only(&c.hyp))
            .map(|c| Named { id: c.id.clone(), hyp: c.hyp })
            .collect();
        if !shared.is_empty() {
            groups.push(shared);
        }
        for c in configs.iter().filter(|c| !field_only(&c.hyp)) {
            groups.push(vec![Named { id: c.id.clone(), hyp: c.hyp }]);
        }
        for group in &groups {
            let tallies = run_pass(&replay, &batches, group, verify);
            for (cfg, t) in group.iter().zip(tallies.into_iter()) {
            writeln!(
                sink,
                concat!(
                    "{{\"file\":{},\"config\":{},\"hyp\":{},",
                    "\"batches\":{},\"content_ok\":{},\"invalid\":{},",
                    "\"parity_shots\":{},\"parity_bad\":{},\"emitted_bad\":{}}}"
                ),
                jstr(&name),
                jstr(&cfg.id),
                jstr(&cfg.hyp.label()),
                t.batches,
                t.content_ok,
                t.invalid,
                t.parity_shots,
                t.parity_bad,
                t.emitted_bad
            )
            .unwrap();
            if let Some(sink) = bsink.as_mut() {
                for r in &t.per_batch {
                    writeln!(
                        sink,
                        concat!(
                            "{{\"file\":{},\"config\":{},\"batch\":{},\"shots\":{},",
                            "\"max_points\":{},\"no_oob\":{},\"noop\":{},\"content_ok\":{}}}"
                        ),
                        jstr(&name),
                        jstr(&cfg.id),
                        r.idx,
                        r.shots,
                        r.max_points,
                        r.no_oob,
                        r.noop,
                        r.ok
                    )
                    .unwrap();
                }
            }
            }
        }
    }
    eprintln!(
        "分片 {}/{}：文件 {}；假设档 {}；nproc 见父进程",
        shard.0,
        shard.1,
        names.len(),
        configs.len()
    );
}
