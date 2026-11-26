//! **半步复检**的实测回归锁（结论见 `docs/04-path-sampling.md` §10）。
//!
//! 这些数字全部是**实测值**，不是标准答案：它们的用处是"采样循环或复检语义一旦被改动，
//! 这里立刻变红"。测量工具就是被 `#[path]` 引进来的 `examples/half_step_probe.rs`
//! ——它把 `src/path.rs::calculate` 的循环逐行复制了一份，只加计数器和变体开关；
//! `probe_matches_production_on_all_fixtures` 负责证明那份复制品与生产逐位相同。

#[path = "../examples/half_step_probe.rs"]
#[allow(dead_code)]
mod probe;

use gw2_rebuild::math_function::parse;
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode as ProdMode};
use gw2_rebuild::graphwar_shot_calculator::{correction, shifted_tokens};
use gw2_rebuild::SERVER_STEP;

use probe::{
    compare_variants, first_divergence, fixture_cases, probe, run_cases, run_probe, sample, Case,
    Mode, Stats, Variant,
};

fn same_point(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 == b.0 || (a.0.is_nan() && b.0.is_nan()))
        && (a.1 == b.1 || (a.1.is_nan() && b.1.is_nan()))
}

/// 点序列逐位相等（NaN 只要求双方都是 NaN，与 `tests/golden.rs` 同口径）。
fn assert_same_points(got: &[(f64, f64)], want: &[(f64, f64)], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}：点数不符");
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert!(same_point(*g, *w), "{what}：第 {i} 点不符 {g:?} vs {w:?}");
    }
}

fn normal_cases() -> Vec<Case> {
    fixture_cases().into_iter().filter(|c| c.mode == Mode::Normal).collect()
}

fn ode_cases() -> Vec<Case> {
    fixture_cases().into_iter().filter(|c| c.mode == Mode::Ode).collect()
}

// ---------------------------------------------------------------------------
// 前置：复制品必须与生产逐位相同（否则下面所有数字都没有意义）
// ---------------------------------------------------------------------------

#[test]
fn probe_matches_production_on_all_fixtures() {
    let cases = fixture_cases();
    assert_eq!(cases.len(), 224, "夹具结构变了");
    let (mut n_normal, mut n_ode) = (0, 0);
    for c in &cases {
        let (variant, prod) = match c.mode {
            Mode::Normal => (Variant::Normal, ProdMode::Normal),
            Mode::Ode => (Variant::Ode, ProdMode::Ode),
        };
        let (got, _) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, variant);
        let want = calculate(&c.tokens, c.x0, c.y0, c.step, c.flip, prod);
        // ① **先**比真值（`c.expected` 来自 `fixtures/path.tsv`，unicorn 直调真采样器）
        assert_same_points(&got, &c.expected, &format!("{} 真值夹具", c.name));
        assert_same_points(&want, &c.expected, &format!("{} 生产 vs 真值夹具", c.name));
        // ② **再**比复制品与生产（自洽；顺序调过来是因为"自洽"不该是第一个判决点）
        assert_same_points(&got, &want, &format!("{} mode={:?}", c.name, c.mode));
        match c.mode {
            Mode::Normal => n_normal += 1,
            Mode::Ode => n_ode += 1,
        }
    }
    assert_eq!((n_normal, n_ode), (112, 112));
}

// ---------------------------------------------------------------------------
// Q1：现有夹具语料上一次都不触发
// ---------------------------------------------------------------------------

#[test]
fn fixture_corpus_never_exercises_the_recheck() {
    let normal = normal_cases();
    assert_eq!(normal.len(), 112);
    let mut steps = 0u64;
    let mut rechecks = 0u64;
    let mut rejects = 0u64;
    for c in &normal {
        let (_, st) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, Variant::Normal);
        steps += st.steps;
        rechecks += st.rechecks;
        rejects += st.rejects;
    }
    // 复检**执行**了近 2000 次（不是没走到这条分支），但一次都没判"不通过"
    assert!(rechecks >= 1900, "复检执行次数异常：{rechecks}");
    assert_eq!(rejects, 0, "夹具语料上出现了复检退回：{rejects}/{rechecks}（{steps} 点）");

    // 换成服务端真实步长 0.01，同样一次都不触发
    let mut rechecks2 = 0u64;
    let mut rejects2 = 0u64;
    let mut steps2 = 0u64;
    for c in &normal {
        let (_, st) = sample(&c.tokens, c.x0, c.y0, SERVER_STEP, c.flip, Variant::Normal);
        steps2 += st.steps;
        rechecks2 += st.rechecks;
        rejects2 += st.rejects;
    }
    assert!(steps2 > 45_000, "点数异常：{steps2}");
    assert!(rechecks2 > 45_000, "复检执行次数异常：{rechecks2}");
    assert_eq!(rejects2, 0, "step=0.01 下出现了复检退回：{rejects2}/{rechecks2}");
}

// ---------------------------------------------------------------------------
// Q2：构造用例上确实触发（Normal），同一条曲线 Ode 不触发
// ---------------------------------------------------------------------------

/// 最小可复现：`sin(60*x)` 从 (0,0)、step=0.01。
///
/// 周期 `2π/60 ≈ 0.105` ⇒ 一个步长内曲线就"上去再回来"，到基点的距离出现峰 ⇒ 复检退回。
/// 阈值在 k≈55：`sin(50*x)` 一次不退，`sin(60*x)` 退 62 次。
#[test]
fn normal_recheck_fires_on_a_constructed_case() {
    let p = probe("sin(60*x)", 0.0, 0.0, SERVER_STEP);
    let (pts, st) = run_probe(&p, Variant::Normal);
    assert!(st.rechecks > 0, "复检根本没执行");
    // 档：**推断**（`62` 是我方模型的测量值，不是二进制里的数）。它的意义是"实现一改就红"，
    // 不是"原版退 62 次"。这条路径上**有真值的是点数组**：
    // `production_matches_the_real_sampler_on_the_constructed_case` 把同一组输入喂给真采样器
    // `0x3c2530`，点数 20001 与 5 个抽样点逐位相同 ⇒ 生产实现没跑偏，62 才有意义。
    assert_eq!(st.rejects, 62, "退回次数与实测不符（{}/{}）", st.rejects, st.rechecks);
    assert_eq!(pts.len(), 20001, "生产点数（真采样器 0x3c2530 也是 20001）");

    // 关掉复检 ⇒ 从第 304 点起走到另一条轨迹（点数相同，点不同）
    let (pts_off, _) = run_probe(&p, Variant::NormalNoRecheck);
    let div = first_divergence(&pts, &pts_off).expect("关掉复检必须改变点序列");
    assert_eq!(div, 304, "分叉点下标与实测不符");
    assert_eq!(pts_off.len(), pts.len(), "这一例的点数不受影响");

    // 同一形状的阈值：k=50 不触发，k=60 触发
    let (_, st50) = run_probe(&probe("sin(50*x)", 0.0, 0.0, SERVER_STEP), Variant::Normal);
    assert_eq!(st50.rejects, 0, "sin(50*x) 本不该触发");
}

/// 同一个构造用例，**生产实现**的点序列必须逐位等于真二进制采样器的输出。
///
/// ## 真值来源（逐位）
///
/// `unicorn` 直调真二进制采样器 `0x3c2530`（`Vec<Vec2> calculate_path(&tokens, x0, y0,
/// step, flip(edx), ecx=mode)`），token 流由真解析器 `0x4ed670` 从 `sin(60*x)` 解出，
/// `x0=0, y0=0, step=0.01, flip=0, mode=0(Normal)`：
///
/// | 项 | 真值 |
/// |---|---|
/// | 点数 | **20001** |
/// | `pts[1]` | `(3f247ae147ae147b, 3f833320c4a0f4d7)` |
/// | `pts[2]` | `(3f347ae147ae147b, 3f9332e97929ed0b)` |
/// | `pts[50]` | `(3f8151eb851eb84e, 3fdf19679d566137)` |
/// | `pts[100]` | `(3f978a3d70a3d6fd, 3fef6a3ff2fd07f3)` |
/// | `pts[20000]` | `(4014a68a3d709db0, 3fee7631b396abbc)` |
///
/// 复算：`.reference/venv/bin/python tools/gen_test_expectations.py`
/// → `fixtures/test_expectation_truth.tsv` 的 `half/H01` 段。
///
/// 改前本文件只有"复制品 == 生产"（自洽）与"退回次数 = 62"（我方模型的数）；
/// `62` 这个数仍是我方模型的产物，但**它现在已经挂在真点数组上了**：
/// 只要生产实现与真二进制差一位，这条就红。
#[test]
fn production_matches_the_real_sampler_on_the_constructed_case() {
    let tokens = parse("sin(60*x)");
    let prod = calculate(&tokens, 0.0, 0.0, SERVER_STEP, false, ProdMode::Normal);
    assert_eq!(prod.len(), 20001, "点数与真采样器 0x3c2530 不一致");
    let bits = |v: f64| v.to_bits();
    let want = [
        (1usize, 0x3f24_7ae1_47ae_147b_u64, 0x3f83_3320_c4a0_f4d7_u64),
        (2, 0x3f34_7ae1_47ae_147b, 0x3f93_32e9_7929_ed0b),
        (50, 0x3f81_51eb_851e_b84e, 0x3fdf_1967_9d56_6137),
        (100, 0x3f97_8a3d_70a3_d6fd, 0x3fef_6a3f_f2fd_07f3),
        (20000, 0x4014_a68a_3d70_9db0, 0x3fee_7631_b396_abbc),
    ];
    for (i, wx, wy) in want {
        assert_eq!(
            (bits(prod[i].0), bits(prod[i].1)),
            (wx, wy),
            "pts[{i}] 与真采样器不一致：ours=({:#018x}, {:#018x})",
            bits(prod[i].0),
            bits(prod[i].1)
        );
    }
}

#[test]
fn ode_never_fires_on_the_same_curve() {
    let p = probe("sin(60*x)", 0.0, 0.0, SERVER_STEP);
    let (pts, st) = run_probe(&p, Variant::Ode);
    assert_eq!(st.rechecks, 0, "生产 Ode 不执行复检");
    assert_eq!(pts.len(), 3104, "Ode 点数");

    // 即便强行给 Ode 加上复检，这条曲线上也一次都不触发 ⇒ 点序列完全相同
    let (pts_r, st_r) = run_probe(&p, Variant::OdeWithRecheck);
    assert!(st_r.rechecks > 3000, "变体复检执行次数异常：{}", st_r.rechecks);
    assert_eq!(st_r.rejects, 0, "Ode 在这条曲线上不该触发：{}", st_r.rejects);
    assert_eq!(first_divergence(&pts, &pts_r), None, "Ode 的点序列不该变");
}

// ---------------------------------------------------------------------------
// Q3：给 Ode 也加复检 —— 在 ODE 夹具上**会变**，能定位到第 N 点
// ---------------------------------------------------------------------------

/// `divx`（`1/x`）从 `x0=-3` 出发、朝 `x=0` 走：第 78 点处生产一步跨过了极点，
/// 加复检后不跨（`lo=x, hi=mid2` 把候选拉回极点这一侧）。
#[test]
fn ode_with_recheck_diverges_at_point_78_on_divx() {
    let cases = ode_cases();
    let c = cases
        .iter()
        .find(|c| {
            c.name == "divx" && !c.flip && c.x0 == -3.0 && c.y0 == 1.0 && c.step == 0.1
        })
        .expect("夹具结构变了：找不到 divx/flip=false/(-3,1)/0.1");

    // 夹具本身 = 真二进制：401 点
    assert_eq!(c.expected.len(), 401, "真值夹具点数");

    let (prod, _) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, Variant::Ode);
    let (alt, st) = sample(&c.tokens, c.x0, c.y0, c.step, c.flip, Variant::OdeWithRecheck);
    assert_eq!(prod.len(), 401);
    assert_eq!(st.rejects, 14, "退回次数与实测不符");
    assert_eq!(alt.len(), 175, "加复检后的点数");
    assert_eq!(first_divergence(&prod, &alt), Some(78), "分叉点下标");

    // 第 78 点前后的对比：生产跨过 x=0，变体留在负半轴
    assert!(prod[78].0 > 0.0, "生产第 78 点应当已跨到 x>0：{:?}", prod[78]);
    assert!(alt[78].0 < 0.0, "加复检后第 78 点应当还在 x<0：{:?}", alt[78]);

    // 全量：112 条 ODE 夹具里，只有 divx / tan（都有极点）会变，共 11 条
    let cmp = compare_variants(&cases, Variant::Ode, Variant::OdeWithRecheck);
    assert_eq!(cmp.cases, 112);
    assert_eq!(cmp.changed, 11, "ODE 夹具里点序列改变的条数");
    // 复检总共只退回 160 次（= divx 43 + tan 117）
    let (total, _) = run_cases(&cases, Variant::OdeWithRecheck, |c| c.name.clone());
    assert_eq!(total.rejects, 160, "ODE 夹具上的总退回次数");

    // 其余 101 条（const0/x/sin/x^2/sqrt/log/ln/exp/abs/div2/negx/const3）一次不退
    let (_, groups) = run_cases(&cases, Variant::OdeWithRecheck, |c| c.name.clone());
    for (name, b) in &groups {
        if name == "divx" || name == "tan" {
            assert!(b.rejects > 0, "{name} 应当触发");
        } else {
            assert_eq!(b.rejects, 0, "{name} 不该触发：{}", b.rejects);
        }
    }
}

// ---------------------------------------------------------------------------
// 文档里那条真实复现：验证探针确实复刻了生产 / 历史 bug 的两个已知数
// ---------------------------------------------------------------------------

/// `tools/bisect_rules.py` 的 doc89 case（`04-path-sampling.md` §8 的验收数字来自它）。
///
/// 三个已知数一个都不能歪：
/// * step=0.5  → 生产 401 点、末点 `(24.8098, 9.8719)`；
/// * step=0.5  → 历史 bug 70 点、停在 `(3.2361, 5.3681)`（"弹道半空截断"）；
/// * step=0.01 → 生产 20001 点、末点 `(23.2128, 6.4176)` ≈ 真游戏爆炸点 23.213。
#[test]
fn documented_real_case_reproduces_both_production_and_the_old_bug() {
    let text = "(abs(cos((-1.43*x)))*8.65)";
    let (x0, y0) = (-1.3401100016801577_f64, 7.565109751327434_f64);
    let raw = parse(text);
    let c = correction(&raw, x0, y0, false);
    let toks = shifted_tokens(&raw, c);
    assert!((c - 4.635142).abs() < 1e-6, "修正常数 c={c}");

    let (prod, st) = sample(&toks, x0, y0, 0.5, false, Variant::Normal);
    assert_eq!(prod.len(), 401, "0.5 步长的点数（docs/04 §8）");
    assert!((prod[400].0 - 24.8098).abs() < 5e-4, "末点 x={}", prod[400].0);
    assert!((prod[400].1 - 9.8719).abs() < 5e-4, "末点 y={}", prod[400].1);
    assert!(st.rejects > 0, "这条曲线在 0.5 步长下必然触发复检");

    let (old, _) = sample(&toks, x0, y0, 0.5, false, Variant::NormalRejectKeepHi);
    assert_eq!(old.len(), 70, "历史 bug 的截断点数");
    assert!((old[69].0 - 3.236053).abs() < 5e-4, "旧 bug 停点 x={}", old[69].0);
    assert!((old[69].1 - 5.368069).abs() < 5e-4, "旧 bug 停点 y={}", old[69].1);

    let (real, st01) = sample(&toks, x0, y0, SERVER_STEP, false, Variant::Normal);
    assert_eq!(real.len(), 20001, "实况步长的点数");
    assert!((real[20000].0 - 23.2128).abs() < 5e-4, "实况末点 x={}", real[20000].0);
    assert!((real[20000].1 - 6.4176).abs() < 5e-4, "实况末点 y={}", real[20000].1);
    assert!(st01.rejects > 0);
    // `chord2` 为 NaN 的退回一次都没有 ⇒ 汇编的 unordered 分支仍未被观测到（§9.3.1）
    assert_eq!(st.rejects_nan, 0);
}

/// 触发率汇总的口径自检：`Stats` 的字段互相自洽（供上面几条断言之外的人工核对）。
#[test]
fn stats_are_self_consistent() {
    let p = probe("sin(60*x)", 0.0, 0.0, SERVER_STEP);
    let (_, st): (_, Stats) = run_probe(&p, Variant::Normal);
    // 每个被接受点都会走到一次复检；多出来的次数是"退回后重来"
    assert!(st.rechecks >= st.steps, "rechecks={} steps={}", st.rechecks, st.steps);
    // 二分次数必然 > 点数（每点至少一次）
    assert!(st.bisect_iters >= st.steps);
    assert!(st.hit_rate() > 0.0 && st.hit_rate() < 1.0);
}
