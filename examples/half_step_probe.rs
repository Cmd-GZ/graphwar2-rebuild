//! **半步复检探针**：量 `src/path.rs` 里那段 Normal-only 的"半步复检"到底多常触发，
//! 以及给 Ode 也加上它之后点序列会不会变。
//!
//! 背景（`docs/04-path-sampling.md` §9）：二分循环在容差 `|L - step| < 0.001` 满足后，
//! **只有 Normal 模式**再算一次 `mid2 = (x+mid)/2` 的弦长；若 `!(L > L2)`（含 NaN）
//! 就退回 `(lo, hi) = (x, mid2)`。Ode 分支（`0x3c2840`）容差满足即接受。
//!
//! ⚠️ 本文件**不改** `src/path.rs`：采样循环在这里逐行复制了一份，只加计数器与变体开关。
//! 复制品与生产的等价性由 `tests/half_step.rs::probe_matches_production` 用 224 条真值夹具
//! 逐点断言（Normal / Ode 各 112 条，逐位相等）。
//!
//! 跑法：
//! ```text
//! cargo run --offline --release --example half_step_probe
//! cargo run --offline --release --example half_step_probe -- --q1
//! cargo run --offline --release --example half_step_probe -- --real <shots.tsv>
//! ```
//!
//! `--real` 的 TSV 由真实回放语料导出，格式与导出方法见
//! `docs/04-path-sampling.md` §10.5（`mode<TAB>flip<TAB>x0<TAB>y0<TAB>func<TAB>回放名`）。

use std::fs;
use std::path::PathBuf;

use gw2_rebuild::math_function::{eval, parse};
use gw2_rebuild::graphwar_shot_calculator::{correction, shifted_tokens};
use gw2_rebuild::{Function, Token, LENGTH_TOL, MAX_ABS_X, MAX_ABS_Y, SERVER_STEP, TOTAL_LEN};

// ---- 与 src/path.rs 同值 -----------------------------------------------------

const HALF: f64 = 0.5;
const SIXTH: f64 = 6.0;
const MAX_BISECT: usize = 100;

/// 弦长。游戏用的是 `sqrt(a*a + b*b)`（`m_hypot`），不是 `f64::hypot`。
pub fn chord_of(a: f64, b: f64) -> f64 {
    (a * a + b * b).sqrt()
}

/// 点数预算，与 `path::point_budget` 同式。
pub fn point_budget(step: f64) -> i32 {
    let n = (TOTAL_LEN / step) as i32;
    if n >= 2 { n } else { 1 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Ode,
}

/// 实验变体。
///
/// | 变体 | 复检 | 退回时 `hi` | 对应 |
/// |---|---|---|---|
/// | [`Variant::Normal`] | 开 | `= mid2` | **生产**（汇编 `0x3c2a0a`） |
/// | [`Variant::NormalNoRecheck`] | 关 | — | 反事实：没有这段代码 |
/// | [`Variant::NormalRejectKeepHi`] | 开 | 不动 | 历史 bug（`04-path-sampling.md` §9.1） |
/// | [`Variant::Ode`] | 关 | — | **生产**（`0x3c2840`） |
/// | [`Variant::OdeWithRecheck`] | 开 | `= mid2` | 反事实：给 Ode 补上复检 |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Normal,
    NormalNoRecheck,
    NormalRejectKeepHi,
    Ode,
    OdeWithRecheck,
}

impl Variant {
    pub fn mode(self) -> Mode {
        match self {
            Variant::Ode | Variant::OdeWithRecheck => Mode::Ode,
            _ => Mode::Normal,
        }
    }

    pub fn recheck(self) -> bool {
        matches!(self, Variant::Normal | Variant::NormalRejectKeepHi | Variant::OdeWithRecheck)
    }

    /// 复检不通过时是否做 `hi = mid2`（生产行为）。
    pub fn rollback_hi(self) -> bool {
        !matches!(self, Variant::NormalRejectKeepHi)
    }

    pub fn label(self) -> &'static str {
        match self {
            Variant::Normal => "Normal(生产)",
            Variant::NormalNoRecheck => "Normal(无复检)",
            Variant::NormalRejectKeepHi => "Normal(旧bug:不改hi)",
            Variant::Ode => "Ode(生产)",
            Variant::OdeWithRecheck => "Ode(+复检)",
        }
    }
}

/// 采样过程中发生的事（本次调用内的计数）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// 被接受的点数（不含首点）。
    pub steps: u64,
    /// 内层二分迭代总次数。
    pub bisect_iters: u64,
    /// 容差满足后执行半步复检的次数。
    pub rechecks: u64,
    /// 复检判为"不通过"（`!(chord > chord2)`，含 NaN）而退回的次数。
    pub rejects: u64,
    /// 其中 `chord2` 是 NaN 的次数（`ucomisd` 的 unordered 分支）。
    pub rejects_nan: u64,
    /// 因 NaN / 越界 / 二分不收敛而没有继续加点（= 生产里的 `return pts`）。
    pub stopped_early: bool,
}

impl Stats {
    pub fn hit_rate(&self) -> f64 {
        if self.rechecks == 0 { 0.0 } else { self.rejects as f64 / self.rechecks as f64 }
    }
}

/// 一次 RK4 步进：从 `(x, y)` 积分到 `mid`，返回 `(y(mid), 弦长)`。
///
/// 与 `src/path.rs` 的 Ode 分支同式（`h = mid - x`，第三变量恒 `0.0`）。
pub fn ode_step(tokens: &[Token], x: f64, y: f64, mid: f64) -> (f64, f64) {
    let h = mid - x;
    let k1 = eval(tokens, x, y, 0.0);
    let k2 = eval(tokens, x + h * HALF, y + k1 * h * HALF, 0.0);
    let k3 = eval(tokens, x + h * HALF, y + k2 * h * HALF, 0.0);
    let k4 = eval(tokens, x + h, y + k3 * h, 0.0);
    let yn = y + (h / SIXTH) * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
    (yn, chord_of(h, yn - y))
}

/// `src/path.rs::calculate` 的**逐行复制** + 计数器 + 变体开关。
pub fn sample(
    tokens: &[Token],
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    v: Variant,
) -> (Vec<(f64, f64)>, Stats) {
    let mut pts = vec![(x0, y0)];
    let mut x = if flip { -x0 } else { x0 };
    let mut y = y0;
    let n = point_budget(step);
    let mut count: i32 = 0;
    let mut st = Stats::default();
    let mode = v.mode();

    loop {
        let mut lo = x;
        let mut hi = x + step;
        let mut accepted = false;

        for _ in 0..MAX_BISECT {
            st.bisect_iters += 1;
            let mid = (lo + hi) * HALF;

            let (cand_y, chord) = match mode {
                Mode::Ode => ode_step(tokens, x, y, mid),
                Mode::Normal => {
                    let fm = eval(tokens, mid, 0.0, 0.0);
                    (fm, chord_of(mid - x, fm - y))
                }
            };

            if !(LENGTH_TOL > (chord - step).abs()) {
                if chord > step || chord.is_nan() {
                    hi = mid;
                } else {
                    lo = mid;
                }
                continue;
            }

            // ---- 被测量的那一段：半步复检 ----------------------------------
            if v.recheck() {
                st.rechecks += 1;
                let mid2 = (x + mid) * HALF;
                let chord2 = match mode {
                    // Ode 的复检：往 mid2 再积分一次（反事实变体）
                    Mode::Ode => ode_step(tokens, x, y, mid2).1,
                    Mode::Normal => {
                        let fm2 = eval(tokens, mid2, 0.0, 0.0);
                        chord_of(mid2 - x, fm2 - y)
                    }
                };
                // 与汇编 `jbe` 同构：`!(chord > chord2)`，NaN 也走退回
                if !(chord > chord2) {
                    st.rejects += 1;
                    if chord2.is_nan() {
                        st.rejects_nan += 1;
                    }
                    lo = x;
                    // 0x3c2a0a：比较之前就无条件写 hi = mid2
                    if v.rollback_hi() {
                        hi = mid2;
                    }
                    continue;
                }
            }
            // ---------------------------------------------------------------

            let px = if flip { -mid } else { mid };
            if cand_y.is_nan() || px.is_nan() {
                st.stopped_early = true;
                return (pts, st);
            }
            if mid.abs() > MAX_ABS_X || cand_y.abs() > MAX_ABS_Y {
                st.stopped_early = true;
                return (pts, st);
            }

            pts.push((px, cand_y));
            count += 1;
            st.steps += 1;
            x = mid;
            y = cand_y;
            accepted = true;
            break;
        }

        if !accepted || count == n {
            st.stopped_early |= !accepted;
            return (pts, st);
        }
    }
}

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

/// `fixtures/path.tsv` 的一行。
#[derive(Debug, Clone)]
pub struct Case {
    /// 表达式名（夹具里只有 token，没有文本）
    pub name: String,
    pub flip: bool,
    pub mode: Mode,
    pub x0: f64,
    pub y0: f64,
    pub step: f64,
    pub tokens: Vec<Token>,
    /// 真二进制的点序列（unicorn 直接调 `0x3c2530`）
    pub expected: Vec<(f64, f64)>,
}

fn hex_f64(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap_or_else(|e| panic!("bad hex {s}: {e}")))
}

fn parse_tokens(s: &str) -> Vec<Token> {
    if s == "-" {
        return Vec::new();
    }
    s.split(';')
        .map(|item| {
            let (tag, bits) = item.split_once(':').expect("token 形如 tag:hexbits");
            let tag = match tag.parse::<u64>().expect("tag 是数字") {
                0 => Function::VarX,
                1 => Function::VarY,
                2 => Function::VarDy,
                3 => Function::Add,
                4 => Function::Sub,
                5 => Function::UnaryMinus,
                6 => Function::Mul,
                7 => Function::Divide,
                8 => Function::Pow,
                9 => Function::Sqrt,
                10 => Function::Log,
                11 => Function::Ln,
                12 => Function::Exp,
                13 => Function::Abs,
                14 => Function::Sin,
                15 => Function::Cos,
                16 => Function::Tan,
                17 => Function::Const,
                18 => Function::LeftBracket,
                19 => Function::RightBracket,
                n => panic!("未知 tag {n}"),
            };
            Token { tag, value: hex_f64(bits) }
        })
        .collect()
}

/// 读 `fixtures/path.tsv`（224 条 = 14 表达式 × 4 参数 × 2 flip × 2 模式）。
pub fn fixture_cases() -> Vec<Case> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("fixtures");
    p.push("path.tsv");
    let text = fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", p.display()));
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let expected: Vec<(f64, f64)> = if f[8] == "-" {
            Vec::new()
        } else {
            f[8].split(';')
                .map(|q| {
                    let (a, b) = q.split_once(',').unwrap();
                    (hex_f64(a), hex_f64(b))
                })
                .collect()
        };
        out.push(Case {
            name: f[0].to_string(),
            flip: f[1] == "1",
            mode: if f[2] == "1" { Mode::Ode } else { Mode::Normal },
            x0: hex_f64(f[3]),
            y0: hex_f64(f[4]),
            step: hex_f64(f[5]),
            tokens: parse_tokens(f[6]),
            expected,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// 统计
// ---------------------------------------------------------------------------

/// 一个分组的汇总。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bucket {
    pub cases: u64,
    pub steps: u64,
    pub bisect_iters: u64,
    pub rechecks: u64,
    pub rejects: u64,
    pub rejects_nan: u64,
}

impl Bucket {
    fn add(&mut self, st: &Stats) {
        self.cases += 1;
        self.steps += st.steps;
        self.bisect_iters += st.bisect_iters;
        self.rechecks += st.rechecks;
        self.rejects += st.rejects;
        self.rejects_nan += st.rejects_nan;
    }

    pub fn rate(&self) -> f64 {
        if self.rechecks == 0 { 0.0 } else { self.rejects as f64 / self.rechecks as f64 }
    }

    pub fn rejects_per_step(&self) -> f64 {
        if self.steps == 0 { 0.0 } else { self.rejects as f64 / self.steps as f64 }
    }
}

/// 跑一批夹具用例、按 `key` 分组。
pub fn run_cases<F>(cases: &[Case], v: Variant, key: F) -> (Bucket, Vec<(String, Bucket)>)
where
    F: Fn(&Case) -> String,
{
    let mut total = Bucket::default();
    let mut groups: Vec<(String, Bucket)> = Vec::new();
    for c in cases {
        let (_, st) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, v);
        total.add(&st);
        let k = key(c);
        match groups.iter_mut().find(|(g, _)| *g == k) {
            Some((_, b)) => b.add(&st),
            None => {
                let mut b = Bucket::default();
                b.add(&st);
                groups.push((k, b));
            }
        }
    }
    (total, groups)
}

/// 两条点序列第一个不同的下标（`None` = 完全相同，含长度）。
pub fn first_divergence(a: &[(f64, f64)], b: &[(f64, f64)]) -> Option<usize> {
    let n = a.len().min(b.len());
    for i in 0..n {
        if !same_point(a[i], b[i]) {
            return Some(i);
        }
    }
    if a.len() != b.len() {
        return Some(n);
    }
    None
}

fn same_point(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 == b.0 || (a.0.is_nan() && b.0.is_nan()))
        && (a.1 == b.1 || (a.1.is_nan() && b.1.is_nan()))
}

/// 两个变体在一批用例上的对比结果。
#[derive(Debug, Clone, Default)]
pub struct Compare {
    pub cases: usize,
    pub changed: usize,
    /// 第一个分叉用例：(名字, 分叉点下标)
    pub first: Option<(String, usize)>,
    /// 变体 B 相对 A 多出的复检 / 退回次数
    pub extra_rechecks: u64,
    pub extra_rejects: u64,
    /// 变体 A / B 的总点数
    pub points_a: u64,
    pub points_b: u64,
}

pub fn compare_variants(cases: &[Case], a: Variant, b: Variant) -> Compare {
    let mut cmp = Compare::default();
    for c in cases {
        let (pa, sa) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, a);
        let (pb, sb) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, b);
        cmp.cases += 1;
        cmp.points_a += sa.steps;
        cmp.points_b += sb.steps;
        cmp.extra_rechecks += sb.rechecks.saturating_sub(sa.rechecks);
        cmp.extra_rejects += sb.rejects.saturating_sub(sa.rejects);
        if let Some(i) = first_divergence(&pa, &pb) {
            cmp.changed += 1;
            if cmp.first.is_none() {
                cmp.first = Some((c.name.clone(), i));
            }
        }
    }
    cmp
}

// ---------------------------------------------------------------------------
// 真实回放语料
// ---------------------------------------------------------------------------

/// 一发真实射击（来自回放语料）。
#[derive(Debug, Clone)]
pub struct RealShot {
    pub mode: Mode,
    pub flip: bool,
    pub x0: f64,
    pub y0: f64,
    pub func: String,
    /// 来源回放名（可选的第 6 列）
    pub src: String,
}

impl RealShot {
    /// 生产路径的 token：Normal 先算修正常数再并进表达式，Ode 原样。
    pub fn tokens(&self) -> Vec<Token> {
        let raw = parse(&self.func);
        if raw.is_empty() {
            return raw;
        }
        match self.mode {
            Mode::Normal => {
                let c = correction(&raw, self.x0, self.y0, self.flip);
                shifted_tokens(&raw, c)
            }
            Mode::Ode => raw,
        }
    }
}

/// 读 `--real` 的 TSV：`mode<TAB>flip<TAB>x0<TAB>y0<TAB>func`。
pub fn load_real_shots(path: &str) -> Vec<RealShot> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {path} 失败: {e}"));
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            RealShot {
                mode: if f[0] == "Ode" { Mode::Ode } else { Mode::Normal },
                flip: f[1] == "1",
                x0: f[2].parse().expect("x0"),
                y0: f[3].parse().expect("y0"),
                func: f.get(4).copied().unwrap_or("").to_string(),
                src: f.get(5).copied().unwrap_or("-").to_string(),
            }
        })
        .collect()
}

/// 真实语料上"生产 vs 变体"的对比。
#[derive(Debug, Clone, Default)]
pub struct RealCompare {
    pub shots: usize,
    /// 至少触发过一次复检的射击数
    pub shots_with_reject: usize,
    /// 单发最多退回次数
    pub max_rejects: u64,
    pub steps: u64,
    pub rechecks: u64,
    pub rejects: u64,
    pub rejects_nan: u64,
    /// 点序列改变的射击数 / 第一个分叉
    pub changed: usize,
    pub first: Option<(usize, String, usize)>,
    /// 变体相对生产的总点数变化（正 = 变体更长）
    pub point_delta: i64,
    /// 变体更短的射击数（"提前停机"）
    pub shortened: usize,
    /// 受影响的回放数（点序列改变的那些发来自多少个回放）
    pub replays_changed: usize,
    /// 语料里一共有多少回放
    pub replays_total: usize,
}

pub fn compare_real(shots: &[RealShot], prod: Variant, other: Variant) -> RealCompare {
    let mut out = RealCompare::default();
    let mut reps_changed: std::collections::BTreeSet<&str> = Default::default();
    let mut reps_all: std::collections::BTreeSet<&str> = Default::default();
    for (idx, s) in shots.iter().enumerate() {
        let toks = s.tokens();
        let (pa, sa) = sample(&toks, s.x0, s.y0, SERVER_STEP, s.flip, prod);
        let (pb, sb) = sample(&toks, s.x0, s.y0, SERVER_STEP, s.flip, other);
        out.shots += 1;
        out.steps += sa.steps;
        out.rechecks += sa.rechecks;
        out.rejects += sa.rejects;
        out.rejects_nan += sa.rejects_nan;
        if sa.rejects > 0 {
            out.shots_with_reject += 1;
            out.max_rejects = out.max_rejects.max(sa.rejects);
        }
        out.point_delta += sb.steps as i64 - sa.steps as i64;
        if sb.steps < sa.steps {
            out.shortened += 1;
        }
        reps_all.insert(s.src.as_str());
        if let Some(i) = first_divergence(&pa, &pb) {
            out.changed += 1;
            reps_changed.insert(s.src.as_str());
            if out.first.is_none() {
                out.first = Some((idx, s.func.clone(), i));
            }
        }
    }
    out.replays_changed = reps_changed.len();
    out.replays_total = reps_all.len();
    out
}

/// 单发明细（用于找"最常触发"的形状）。
#[derive(Debug, Clone)]
pub struct ShotStat {
    pub idx: usize,
    pub func: String,
    pub steps_prod: u64,
    pub steps_alt: u64,
    pub rejects_prod: u64,
    pub rejects_alt: u64,
    pub rejects_nan_alt: u64,
    pub div: Option<usize>,
}

pub fn shot_stats(shots: &[RealShot], prod: Variant, alt: Variant) -> Vec<ShotStat> {
    shots
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let toks = s.tokens();
            let (pa, sa) = sample(&toks, s.x0, s.y0, SERVER_STEP, s.flip, prod);
            let (pb, sb) = sample(&toks, s.x0, s.y0, SERVER_STEP, s.flip, alt);
            ShotStat {
                idx,
                func: s.func.clone(),
                steps_prod: sa.steps,
                steps_alt: sb.steps,
                rejects_prod: sa.rejects,
                rejects_alt: sb.rejects,
                rejects_nan_alt: sb.rejects_nan,
                div: first_divergence(&pa, &pb),
            }
        })
        .collect()
}

/// 退回次数的粗分布（看触发是"个别病态"还是"广谱"）。
fn print_histogram(label: &str, rejects: impl Iterator<Item = u64>) {
    let mut b = [0usize; 5];
    for r in rejects {
        if r == 1 {
            b[0] += 1;
        } else if r <= 10 {
            b[1] += 1;
        } else if r <= 100 {
            b[2] += 1;
        } else if r <= 1000 {
            b[3] += 1;
        } else {
            b[4] += 1;
        }
    }
    println!(
        "    退回次数分布（{}）：1 次 {} 发 | 2–10 {} 发 | 11–100 {} 发 | 101–1000 {} 发 | >1000 {} 发",
        label,
        b[0],
        b[1],
        b[2],
        b[3],
        b[4]
    );
}

fn print_top(stats: &mut Vec<ShotStat>, prod: Variant, alt: Variant, n: usize) {
    stats.sort_by_key(|s| std::cmp::Reverse(s.rejects_alt.max(s.rejects_prod)));
    println!("\n    触发最多的 {} 发（{} → {}）：", n, prod.label(), alt.label());
    for s in stats.iter().take(n) {
        println!(
            "      #{:<4} 退回(生产) {:>5} 退回(变体) {:>5} 其中NaN {:>5} | 点数 {}→{} 分叉 {:?} | {}",
            s.idx, s.rejects_prod, s.rejects_alt, s.rejects_nan_alt,
            s.steps_prod, s.steps_alt, s.div, truncate(&s.func, 60)
        );
    }
}

// ---------------------------------------------------------------------------
// 报告
// ---------------------------------------------------------------------------

fn section(title: &str) {
    println!("\n════════════════════════════════════════════════════════════");
    println!("{title}");
    println!("════════════════════════════════════════════════════════════");
}

fn print_group_table(title: &str, groups: &[(String, Bucket)]) {
    println!("\n  {title}");
    println!(
        "    {:<8} {:>5} {:>9} {:>12} {:>10} {:>12} {:>12}",
        "名字", "用例", "点数", "复检执行", "退回", "退回/复检", "退回/点"
    );
    for (name, b) in groups {
        println!(
            "    {:<8} {:>5} {:>9} {:>12} {:>10} {:>12.4} {:>12.6}",
            name, b.cases, b.steps, b.rechecks, b.rejects, b.rate(), b.rejects_per_step()
        );
    }
}

/// 一条探测用例：文本 + 起点 + 步长。
#[derive(Debug, Clone)]
pub struct Probe {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub step: f64,
}

pub fn probe(text: &str, x0: f64, y0: f64, step: f64) -> Probe {
    Probe { text: text.into(), x0, y0, step }
}

/// 在给定变体下跑一条文本用例（不做修正：起点须在曲线上）。
pub fn run_probe(p: &Probe, v: Variant) -> (Vec<(f64, f64)>, Stats) {
    let toks = parse(&p.text);
    sample(&toks, p.x0, p.y0, p.step, false, v)
}

fn round4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}

fn q1(fixtures: &[Case]) {
    let normal: Vec<Case> = fixtures.iter().filter(|c| c.mode == Mode::Normal).cloned().collect();
    section("Q1 · Normal 半步复检触发率（fixtures/path.tsv，112 条 Normal）");
    let (total, groups) = run_cases(&normal, Variant::Normal, |c| c.name.clone());
    println!(
        "  {} 用例 / {} 接受点 / 二分 {} 次 / 复检执行 {} 次 / 退回 {} 次",
        total.cases, total.steps, total.bisect_iters, total.rechecks, total.rejects
    );
    println!(
        "  触发率 = 退回/复检 = {:.6}    退回/点 = {:.8}",
        total.rate(),
        total.rejects_per_step()
    );
    print_group_table("按表达式：", &groups);

    // 真实步长 0.01 的同一批表达式
    let mut at_server = Vec::new();
    for c in &normal {
        let mut d = c.clone();
        d.step = SERVER_STEP;
        at_server.push(d);
    }
    let (t2, g2) = run_cases(&at_server, Variant::Normal, |c| c.name.clone());
    section("Q1b · 同样 14 个表达式，步长换成服务端 0.01");
    println!(
        "  {} 用例 / {} 接受点 / 二分 {} 次 / 复检执行 {} 次 / 退回 {} 次",
        t2.cases, t2.steps, t2.bisect_iters, t2.rechecks, t2.rejects
    );
    println!(
        "  触发率 = 退回/复检 = {:.6}    退回/点 = {:.8}",
        t2.rate(),
        t2.rejects_per_step()
    );
    print_group_table("按表达式：", &g2);

    // 历史 bug 在夹具上有没有差别
    let cmp = compare_variants(&normal, Variant::Normal, Variant::NormalRejectKeepHi);
    println!(
        "\n  历史 bug（复检退回时不改 hi）在夹具上：{}/{} 例点序列改变（点数 {} → {}）",
        cmp.changed, cmp.cases, cmp.points_a, cmp.points_b
    );
}

fn q2(_fixtures: &[Case]) {
    section("Q2 · 什么形状会触发（step=0.01 候选扫描）");
    let starts = [(0.0, 0.0), (1.0, 0.0), (-3.0, 1.0), (4.0, -2.0), (10.0, 5.0), (1.5, 0.0), (0.1, 0.0)];
    let texts = [
        "x", "x^2", "x^3", "sin(x)", "cos(x)", "tan(x)", "abs(x)", "sqrt(x)", "log(x)",
        "ln(x)", "exp(x)", "1/x", "sin(10*x)", "sin(100*x)", "sin(1000*x)", "sin(1/x)",
        "x*sin(1/x)", "abs(sin(10*x))", "tan(y)", "y", "exp(y)", "y^2", "1/(x-1)",
        "sin(200*x)", "cos(500*x)", "x^2*sin(1000*x)", "10*sin(100*x)",
        // 真实抓包里的形状（docs/127、docs/89）
        "abs(cos((-1.43*x)))*8.65", "cos((0.73*x))", "(-46.06/(7.94+x))",
    ];
    let mut hits = Vec::new();
    let mut zero = 0usize;
    for t in texts {
        for (x0, y0) in starts {
            let (_, st) = run_probe(&probe(t, x0, y0, SERVER_STEP), Variant::Normal);
            if st.rejects > 0 {
                hits.push((t.to_string(), x0, y0, st));
            } else {
                zero += 1;
            }
        }
    }
    println!(
        "  候选 {} 条（{} 表达式 × {} 起点），触发 {} 条，不触发 {} 条：",
        texts.len() * starts.len(),
        texts.len(),
        starts.len(),
        hits.len(),
        zero
    );
    println!("\n    {:<30} {:>16} {:>8} {:>9} {:>8}", "表达式", "起点", "点数", "复检", "退回");
    for (t, x0, y0, st) in &hits {
        println!(
            "    {:<30} ({:>6.2},{:>6.2}) {:>8} {:>9} {:>8}",
            t, x0, y0, st.steps, st.rechecks, st.rejects
        );
    }

    // 让 sin(k*x) 逐步变密，找最小的 k
    section("Q2b · 最小可复现：sin(k*x) 从 (0,0) 出发，k 从稀到密（step=0.01）");
    for k in [5.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 80.0, 100.0] {
        let text = format!("sin({k}*x)");
        let (_, st) = run_probe(&probe(&text, 0.0, 0.0, SERVER_STEP), Variant::Normal);
        println!(
            "    {:<16} 点数 {:>6}  复检 {:>7}  退回 {:>6}",
            text, st.steps, st.rechecks, st.rejects
        );
    }

    // 文件里已记载的那条真实复现
    section("Q2c · 仓库里已记录的真实复现（tools/bisect_rules.py 的 doc89 case）");
    let text = "(abs(cos((-1.43*x)))*8.65)";
    let x0 = -1.3401100016801577;
    let y0 = 7.565109751327434;
    let raw = parse(text);
    let c = correction(&raw, x0, y0, false);
    let toks = shifted_tokens(&raw, c);
    println!("    f(x) = {text}，起点 ({x0}, {y0})，修正 c = {c:.6}，flip = false");
    for step in [0.01, 0.1, 0.5] {
        let (pts, stats) = sample(&toks, x0, y0, step, false, Variant::Normal);
        let (pts_old, _) = sample(&toks, x0, y0, step, false, Variant::NormalRejectKeepHi);
        println!(
            "    step={:<5} 生产 {} 点(末点 {:?}) 复检 {} 退回 {} | 旧bug {} 点(末点 {:?})",
            step,
            pts.len(),
            pts.last().map(|p| (round4(p.0), round4(p.1))),
            stats.rechecks,
            stats.rejects,
            pts_old.len(),
            pts_old.last().map(|p| (round4(p.0), round4(p.1)))
        );
    }
}

fn q3(fixtures: &[Case]) {
    section("Q3 · 给 Ode 也加半步复检，点序列会变吗");
    let ode: Vec<Case> = fixtures.iter().filter(|c| c.mode == Mode::Ode).cloned().collect();
    let (tot, _) = run_cases(&ode, Variant::Ode, |c| c.name.clone());
    let (tot_r, groups_r) = run_cases(&ode, Variant::OdeWithRecheck, |c| c.name.clone());
    println!(
        "  ODE 夹具 {} 条：生产 {} 点（二分 {} 次）；Ode+复检 {} 点（复检执行 {} 次、退回 {} 次）",
        ode.len(), tot.steps, tot.bisect_iters, tot_r.steps, tot_r.rechecks, tot_r.rejects
    );
    print_group_table("按表达式（Ode+复检）：", &groups_r);
    let cmp = compare_variants(&ode, Variant::Ode, Variant::OdeWithRecheck);
    println!(
        "\n  对比：{}/{} 例点序列改变，首个分叉 {:?}",
        cmp.changed, cmp.cases, cmp.first
    );

    // 步长 0.01
    let mut at_server = Vec::new();
    for c in &ode {
        let mut d = c.clone();
        d.step = SERVER_STEP;
        at_server.push(d);
    }
    let cmp2 = compare_variants(&at_server, Variant::Ode, Variant::OdeWithRecheck);
    let (t0, _) = run_cases(&at_server, Variant::Ode, |c| c.name.clone());
    let (t1, _) = run_cases(&at_server, Variant::OdeWithRecheck, |c| c.name.clone());
    println!(
        "  step=0.01：{} 点 → {} 点，复检执行 {} 次、退回 {} 次；{}/{} 例改变，首个分叉 {:?}",
        t0.steps, t1.steps, t1.rechecks, t1.rejects, cmp2.changed, cmp2.cases, cmp2.first
    );

    // 逐例分叉明细（夹具里能被复检改掉的到底是哪几条）
    println!("\n  逐例分叉明细（Ode 夹具，步长用夹具原值）：");
    for c in &ode {
        let (pa, _) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, Variant::Ode);
        let (pb, sb) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, Variant::OdeWithRecheck);
        if let Some(i) = first_divergence(&pa, &pb) {
            println!(
                "    {:<8} flip={:<5} x0={:<6.2} y0={:<6.2} step={:<5} 点数 {}→{} 退回 {} 分叉@{i}",
                c.name, c.flip, c.x0, c.y0, c.step, pa.len(), pb.len(), sb.rejects
            );
        }
    }

    // 病态 ODE 场：看复检在 Ode 里到底有没有可能触发
    section("Q3b · 病态 ODE 场（超出夹具范围；只为看复检在 Ode 里是否可能触发）");
    // 右侧两类：**刚性**（|∂f/∂y|·step 超过 RK4 稳定域 ≈2.78）与**奇点**（一步内越过极点）。
    // 注意 `100*y` 从 (0,0) 出发是平衡点（y 恒为 0），拿它测刚性是无效的；
    // 刚性要用 (0,1) 这种被驱动的起点，`-100*y`（|λ|h≈1，稳定）不触发、`-1000*y`（|λ|h≈10）触发。
    let pathological = [
        ("x", 0.0, 0.0),
        ("x^2", 0.0, 0.0),
        ("exp(x)", 0.0, 0.0),
        ("y", 0.0, 0.0),
        ("y^2", 0.0, 0.0),
        ("exp(y)", -3.0, 1.0),
        ("tan(y)", 0.0, 0.0),
        ("sin(1000*x)", 0.0, 0.0),
        ("x*y", 0.0, 0.0),
        ("y-x", 0.0, 0.0),
        ("-100*y", 0.0, 1.0),      // 稳定：|λ|·step ≈ 1
        ("-1000*y", 0.0, 1.0),     // 刚性：|λ|·step ≈ 10
        ("4096*(-y-x)", 0.0, 1.0), // 真实语料里那两条"4096(-y-x)"的形状
        ("tan(x)", 1.4, 0.0),      // 冲向 π/2 极点
        ("1/(x-1)", 0.5, 0.0),     // 冲向 x=1 极点
        ("1/(x-1)", 1.5, 0.0),     // 背离极点
        ("1/x", 0.001, 0.0),       // 背离极点
        ("ln(x)", 0.0001, 0.0),
        ("sqrt(x)", 0.0001, 0.0),
        ("1/(y-1)", 0.0, 0.0),
    ];
    for (text, x0, y0) in pathological {
        let p = probe(text, x0, y0, SERVER_STEP);
        let (pa, _sa) = run_probe(&p, Variant::Ode);
        let (pb, sb) = run_probe(&p, Variant::OdeWithRecheck);
        println!(
            "    {:<14} x0={:<8} 点数 {:>5}→{:<5} 复检 {:>6} 退回 {:>5} 分叉 {:?}",
            text, x0, pa.len(), pb.len(), sb.rechecks, sb.rejects,
            first_divergence(&pa, &pb)
        );
    }
}

fn real_report(shots: &[RealShot]) {
    let normal: Vec<RealShot> = shots.iter().filter(|s| s.mode == Mode::Normal).cloned().collect();
    let ode: Vec<RealShot> = shots.iter().filter(|s| s.mode == Mode::Ode).cloned().collect();
    section(&format!(
        "真实回放语料：{} 发（Normal {} / DiffEq {}），step=0.01",
        shots.len(),
        normal.len(),
        ode.len()
    ));

    if !normal.is_empty() {
        println!("\n  ── Normal 分支：生产行为下复检的触发 ──");
        let cmp = compare_real(&normal, Variant::Normal, Variant::Normal);
        println!(
            "    {} 发 / {} 接受点 / 复检执行 {} 次 / 退回 {} 次",
            cmp.shots, cmp.steps, cmp.rechecks, cmp.rejects
        );
        println!(
            "    触发率 = {:.6}（退回/复检）   退回/点 = {:.6}   其中 chord2=NaN 的退回 {} 次",
            cmp.rejects as f64 / cmp.rechecks.max(1) as f64,
            cmp.rejects as f64 / cmp.steps.max(1) as f64,
            cmp.rejects_nan
        );
        println!(
            "    至少触发一次的射击：{} / {} = {:.2}%；单发最多退回 {} 次",
            cmp.shots_with_reject,
            cmp.shots,
            100.0 * cmp.shots_with_reject as f64 / cmp.shots.max(1) as f64,
            cmp.max_rejects
        );

        let bug = compare_real(&normal, Variant::Normal, Variant::NormalRejectKeepHi);
        println!(
            "    历史 bug（退回时只 lo=x、不改 hi）：{} / {} 发点序列改变，总点数变化 {:+}，变短 {} 发",
            bug.changed, bug.shots, bug.point_delta, bug.shortened
        );
        println!(
            "      （{} / {} 个回放，{} 发）",
            bug.replays_changed, bug.replays_total, bug.changed
        );
        if let Some((i, f, at)) = &bug.first {
            println!("    首个分叉：第 {} 发（从第 {} 点起），函数 = {}", i, at, truncate(f, 70));
        }

        let off = compare_real(&normal, Variant::Normal, Variant::NormalNoRecheck);
        println!(
            "    完全去掉复检：{} / {} 发改变，总点数变化 {:+}",
            off.changed, off.shots, off.point_delta
        );

        let mut stats = shot_stats(&normal, Variant::Normal, Variant::Normal);
        print_histogram("生产 Normal", stats.iter().map(|s| s.rejects_prod));
        let changed_small = stats.iter().filter(|s| s.div.is_some() && s.rejects_prod <= 10).count();
        println!("    其中「改变时退回 ≤10 次」的：{} 发", changed_small);
        print_top(&mut stats, Variant::Normal, Variant::Normal, 8);
    }

    if !ode.is_empty() {
        println!("\n  ── DiffEq 分支：给 Ode 也加复检（反事实）──");
        let prod = compare_real(&ode, Variant::Ode, Variant::Ode);
        let run = compare_real(&ode, Variant::OdeWithRecheck, Variant::OdeWithRecheck);
        println!(
            "    生产 Ode：{} 发 / {} 接受点（不执行复检）",
            prod.shots, prod.steps
        );
        println!(
            "    Ode+复检：{} 接受点 / 复检执行 {} 次 / 退回 {} 次（其中 NaN {} 次）",
            run.steps, run.rechecks, run.rejects, run.rejects_nan
        );
        println!(
            "    至少触发一次的射击：{} / {} = {:.2}%；单发最多退回 {} 次",
            run.shots_with_reject,
            run.shots,
            100.0 * run.shots_with_reject as f64 / run.shots.max(1) as f64,
            run.max_rejects
        );
        let cmp = compare_real(&ode, Variant::Ode, Variant::OdeWithRecheck);
        println!(
            "    加复检 vs 生产：{} / {} 发点序列改变，总点数变化 {:+}，变短 {} 发",
            cmp.changed, cmp.shots, cmp.point_delta, cmp.shortened
        );
        println!(
            "    受影响回放：{} / {} 个；{} 发改变",
            cmp.replays_changed, cmp.replays_total, cmp.changed
        );
        if let Some((i, f, at)) = &cmp.first {
            println!("    首个分叉：第 {} 发（从第 {} 点起），函数 = {}", i, at, truncate(f, 70));
        }

        let mut stats = shot_stats(&ode, Variant::Ode, Variant::OdeWithRecheck);
        print_histogram("Ode+复检", stats.iter().map(|s| s.rejects_alt));
        let changed_small = stats.iter().filter(|s| s.div.is_some() && s.rejects_alt <= 10).count();
        println!("    其中「改变时退回 ≤10 次」的：{} 发", changed_small);
        print_top(&mut stats, Variant::Ode, Variant::OdeWithRecheck, 8);
    }
}

fn fidelity(fixtures: &[Case]) {
    section("复制品 vs 生产代码（src/path.rs）—— 复制保真度");
    for (label, v, prod) in [
        ("Normal", Variant::Normal, gw2_rebuild::graphwar_path_calculator::Mode::Normal),
        ("Ode", Variant::Ode, gw2_rebuild::graphwar_path_calculator::Mode::Ode),
    ] {
        let mut bad = 0;
        let mut n = 0;
        for c in fixtures.iter().filter(|c| {
            (c.mode == Mode::Normal) == (prod == gw2_rebuild::graphwar_path_calculator::Mode::Normal)
        }) {
            let (p, _) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, v);
            let q = gw2_rebuild::graphwar_path_calculator::calculate(&c.tokens, c.x0, c.y0, c.step, c.flip, prod);
            n += 1;
            if first_divergence(&p, &q).is_some() || p.len() != q.len() {
                bad += 1;
            }
        }
        println!("  {label}: {n} 例，逐位不一致 {bad} 例");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run_all = args.is_empty();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let real_path = args.iter().position(|a| a == "--real").and_then(|i| args.get(i + 1)).cloned();

    let fixtures = fixture_cases();
    println!(
        "夹具 {} 条（Normal {} / Ode {}）",
        fixtures.len(),
        fixtures.iter().filter(|c| c.mode == Mode::Normal).count(),
        fixtures.iter().filter(|c| c.mode == Mode::Ode).count()
    );

    if run_all || has("--fidelity") {
        fidelity(&fixtures);
    }
    if run_all || has("--q1") {
        q1(&fixtures);
    }
    if run_all || has("--q2") {
        q2(&fixtures);
    }
    if run_all || has("--q3") {
        q3(&fixtures);
    }
    if let Some(path) = real_path {
        let shots = load_real_shots(&path);
        real_report(&shots);
    }
}
