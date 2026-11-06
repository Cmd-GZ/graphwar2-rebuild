//! 回放对拍**收敛**层的验收（`docs/142-diff-converge.md`）。
//!
//! 每个用例要么钉住一条**从回放里读出来的真值**（逐位验证），要么是它的**负样本**
//! （证明这条规则不是"怎么写都算过"）。语料相关的那条在没有语料时会打印原因跳过，
//! 不静默通过（和 `tests/replay_diff.rs::real_corpus_smoke_if_present` 同一约定）。

use gw2_rebuild::diffconv::{
    corrected_function, death_effect_type, explosion_point, mode_of, strict_policy,
    unbalanced_brackets, ConvPredictor,
};
use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::replaydiff::{default_corpus_dir, Msg, Policy};
use gw2_rebuild::settle::Target;

/// 回放里的一行 `FunctionInfo` / `EffectInfo` 原文，按同一口径解析。
fn kinds(msgs: &[String]) -> Vec<String> {
    msgs.iter()
        .map(|w| Msg::parse_wire(w).map(|m| m.kind.clone()).unwrap_or_else(|_| "<非法>".into()))
        .collect()
}

fn field(msgs: &[String], idx: usize, name: &str) -> String {
    Msg::parse_wire(&msgs[idx]).unwrap().get_str(name).unwrap_or_default()
}

fn fval(msgs: &[String], idx: usize, name: &str) -> f64 {
    Msg::parse_wire(&msgs[idx]).unwrap().get_f64(name).unwrap_or(f64::NAN)
}

// ---------------------------------------------------------------------------
// 1. 采样模式跟着 `FunctionModeInfo` 走
// ---------------------------------------------------------------------------

#[test]
fn mode_follows_function_mode_string() {
    assert_eq!(mode_of("DiffEqFunction"), Mode::Ode);
    assert_eq!(mode_of("NormalFunction"), Mode::Normal);
    // 未知值按 Normal：不猜（`docs/136` §3 的语料里只有这两个值）
    assert_eq!(mode_of("something-else"), Mode::Normal);
    // 负样本：DiffEq 不能被当成 Normal
    assert_ne!(mode_of("DiffEqFunction"), Mode::Normal);
}

// ---------------------------------------------------------------------------
// 2. `corrected_function` 的规则（全语料 740 Normal / 550 DiffEq 逐条核对）
// ---------------------------------------------------------------------------

/// DiffEq 局：`corrected_function` **就是**函数原文，一个字符都不加。
///
/// 真值：回放 `1782760054` 的 `FunctionInfo`（全语料 DiffEq 550/550 零例外）。
#[test]
fn diffeq_corrected_function_has_no_suffix() {
    let f = "-222(-y+(-5+(13)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(20))))))))";
    assert_eq!(corrected_function(f, 3.0, 4.0, false, Mode::Ode), f);
    // 负样本：加上 Normal 的后缀就不再是真值（这正是 `Match::fire` 旧行为）
    assert_ne!(
        corrected_function(f, 3.0, 4.0, false, Mode::Ode),
        corrected_function(f, 3.0, 4.0, false, Mode::Normal)
    );
    // 空函数也必须保持空（而不是 " + 0.000"）
    assert_eq!(corrected_function("", 3.0, 4.0, false, Mode::Ode), "");
}

/// Normal 局：函数原文 + `" ± c.ddd"`（运算符带符号，`docs/127`）。
///
/// 真值：抓包帧 234 / 231 / 182（`src/shot.rs` 的同一批样本）。
#[test]
fn normal_corrected_function_appends_signed_suffix() {
    assert_eq!(
        corrected_function("0.00", 1.651220991201045, 4.638020825690482, false, Mode::Normal),
        "0.00 + 4.638"
    );
    assert_eq!(
        corrected_function("", -13.429861913415694, -3.686102115928783, false, Mode::Normal),
        " - 3.686"
    );
    // 负样本：c < 0 时不能写 "+ -"
    assert!(!corrected_function("", -13.429861913415694, -3.686102115928783, false, Mode::Normal)
        .contains("+ -"));
}

/// 括号没闭合的输入：真值**补齐右括号**再拼后缀。
///
/// 真值：回放 `1782642986` —— `function = "5/155sin(4exp(4*6cos("`、
/// `corrected_function = "5/155sin(4exp(4*6cos())) + 6.055"`（全语料 11 条同类）。
#[test]
fn unbalanced_brackets_are_closed_like_the_game() {
    assert_eq!(unbalanced_brackets("5/155sin(4exp(4*6cos("), 3);
    assert_eq!(unbalanced_brackets("sin(x)"), 0);
    // 多余的右括号不参与（下限截到 0）
    assert_eq!(unbalanced_brackets("sin(x)))"), 0);
    // 真值：回放 `1782642986` 第 457 行的 `FunctionInfo`；
    // 同一批的 `FunctionPoints`（第 460 行）给出 start = (-2.632134919326489, 6.05141758173637)
    let got = corrected_function(
        "5/155sin(4exp(4*6cos(",
        -2.632134919326489,
        6.05141758173637,
        false,
        Mode::Normal,
    );
    assert_eq!(got, "5/155sin(4exp(4*6cos())) + 6.055");
    // 负样本：不加括号就与真值不同
    assert_ne!(got, "5/155sin(4exp(4*6cos( + 6.055");
}

// ---------------------------------------------------------------------------
// 3. 爆炸点：不是采样器的停点，而是"修正文本在 end_x 处求值"
// ---------------------------------------------------------------------------

/// 真值：回放 `1782640359`（Normal、单发、飞到上边界）
/// * `function  = "(40)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(25))))))"`
/// * `corrected = "… - 0.297"`、`start=(-2.1413650290582837,-0.29656732812543574)`
/// * `FunctionPoints.end_x = 4.988566611566738`
/// * `EffectInfo(Explosion).pos_y = 14.991174112697316`（回放原文）
///
/// 我们采样器停点的 y 是 `14.99160678457188` —— **差 4.3e-4**，所以"停点即爆炸点"
/// 这条是错的；把修正文本回代，逐位对上。
#[test]
fn explosion_point_is_the_corrected_text_at_end_x() {
    let f = "(40)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(25))))))";
    let (x0, y0) = (-2.1413650290582837_f64, -0.29656732812543574_f64);
    let end_x = 4.988566611566738_f64;
    let corrected = corrected_function(f, x0, y0, false, Mode::Normal);
    assert_eq!(
        corrected,
        "(40)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(25)))))) - 0.297"
    );
    let sampler_stop = (end_x, 14.99160678457188_f64); // 我们采样器的最后一个点
    let (px, py) = explosion_point(&corrected, sampler_stop, false, Mode::Normal);
    assert_eq!(px.to_bits(), end_x.to_bits(), "爆炸点 x 就是 end_x");
    assert_eq!(py, 14.991174112697316, "爆炸点 y = 修正文本在 end_x 处的值");
    // 负样本：停点本身不是爆炸点（4.3e-4 的差不能当成"末位差"放过）
    assert_ne!(py.to_bits(), sampler_stop.1.to_bits());
    assert!((sampler_stop.1 - py).abs() > 1e-5);
}

/// `flip` 时采样器内部用 `-x`，回代也要取反（`graphwar_path_calculator::calculate` 的 `px = -mid`）。
///
/// 真值：回放 `1782641987` —— `function = "sin(x)"`、`corrected = "sin(x) - 6.101"`、
/// `start_x = 14.559477538378344`（Team2 ⇒ flip=true）、`end_x = 8.746977538378527`、
/// `pos_y = -6.7280811727617085`。
#[test]
fn explosion_point_honours_flip() {
    let corrected = "sin(x) - 6.101";
    let stop = (8.746977538378527_f64, -6.728536236129325_f64);
    let (_, right) = explosion_point(corrected, stop, false, Mode::Normal);
    let (_, flipped) = explosion_point(corrected, stop, true, Mode::Normal);
    assert_eq!(flipped, -6.7280811727617085, "flip 时按 -x 回代");
    // 负样本：不取反会得到完全不同的值（差 1.25 以上）
    assert!((right - flipped).abs() > 1.0);
}

/// DiffEq 不走这条回代：那时 y 不是纯函数值，停点自己就是真值。
#[test]
fn explosion_point_keeps_the_stop_for_ode() {
    let stop = (1.5_f64, -2.5_f64);
    assert_eq!(explosion_point("-222(-y+(-5+(13)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(20))))))))", stop, false, Mode::Ode), stop);
}

// ---------------------------------------------------------------------------
// 4. 死亡特效的种类由**皮肤**决定
// ---------------------------------------------------------------------------

/// 真值（全语料 936 条逐条按坐标配对）：`target` → SparkEffect（54/54）、
/// `bot_*` → AshEffect（363/368）、人类皮肤 → DeathEffect（514/514）。
#[test]
fn death_effect_type_follows_the_skin() {
    assert_eq!(death_effect_type(Some("target")), "SparkEffect");
    assert_eq!(death_effect_type(Some("bot_0")), "AshEffect");
    assert_eq!(death_effect_type(Some("bot_5")), "AshEffect");
    assert_eq!(death_effect_type(Some("yellow")), "DeathEffect");
    assert_eq!(death_effect_type(Some("green")), "DeathEffect");
    // 没有皮肤信息时退回 DeathEffect（不猜）
    assert_eq!(death_effect_type(None), "DeathEffect");
    // 负样本：这是三条不同的字符串，不能塌成一条
    assert_ne!(death_effect_type(Some("target")), death_effect_type(Some("bot_0")));
    assert_ne!(death_effect_type(Some("bot_0")), death_effect_type(Some("yellow")));
}

/// 收敛层的口径是**收紧**：Ash/Spark 现在算"我方产出"，因此**要**被比。
#[test]
fn strict_policy_compares_ash_and_spark() {
    let p: Policy = strict_policy();
    for et in ["ExplosionEffect", "DeathEffect", "AshEffect", "SparkEffect"] {
        assert!(p.produced_effects.contains(&et), "{et} 必须被比较");
    }
    let msg = Msg::parse_wire(
        r#"{"EffectInfo":{"effect_id":1,"pos_x":0.0,"pos_y":0.0,"effect_type":"AshEffect","start_tick":1}}"#,
    )
    .unwrap();
    assert!(!p.is_unproduced(&msg), "AshEffect 不能再被当成「我方不产」整类排除");
}

// ---------------------------------------------------------------------------
// 5. 报文顺序与字段（用预测器本身产一遍，真值 = 抓包顺序 `docs/43`）
// ---------------------------------------------------------------------------

#[test]
fn predictor_emits_the_captured_window_in_order() {
    // 一个"飞到边界、不打死人"的回合：函数 0.00 ⇒ 水平直线
    let mut p = ConvPredictor::new(&[], vec![Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 }], Mode::Normal);
    let (msgs, out) = p.fire(1, "0.00", 0.0, 0.0, false, 100);
    assert_eq!(
        kinds(&msgs),
        vec![
            "GameStateInfo",
            "FunctionInfo",
            "FunctionPoints",
            "GameStateInfo",
            "FunctionActive",
            "EffectInfo",
            "GameStateInfo",
        ]
    );
    assert_eq!(field(&msgs, 1, "corrected_function"), "0.00 + 0.000");
    assert_eq!(field(&msgs, 5, "effect_type"), "ExplosionEffect");
    assert!(!out.blocked, "无障碍 ⇒ 自由飞行");
    // 爆炸点按修正文本回代：直线 ⇒ y = 0.000 常数
    assert_eq!(fval(&msgs, 5, "pos_y").to_bits(), 0.0_f64.to_bits());
    assert_eq!(fval(&msgs, 5, "pos_x").to_bits(), out.stop.0.to_bits());
}

/// 负样本：把一个 `bot_0` 士兵摆在弹道上 ⇒ 必须**多出**一条 `AshEffect`，
/// 而且位置就是那名士兵的坐标（不是爆炸点）。
#[test]
fn killed_bot_gets_an_ash_effect_at_its_own_position() {
    let mut p = ConvPredictor::new(
        &[],
        vec![
            Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 },
            Target { id: 7, x: 5.0, y: 0.0, radius: 0.5 },
        ],
        Mode::Normal,
    );
    p.skin.insert(7, "bot_0".into());
    let (msgs, out) = p.fire(1, "0.00", 0.0, 0.0, false, 100);
    assert_eq!(out.killed, vec![7]);
    let k = kinds(&msgs);
    assert_eq!(
        k,
        vec![
            "GameStateInfo",
            "FunctionInfo",
            "FunctionPoints",
            "GameStateInfo",
            "LifeQueuedDeath",
            "EffectInfo",
            "FunctionActive",
            "EffectInfo",
            "GameStateInfo",
        ]
    );
    assert_eq!(field(&msgs, 5, "effect_type"), "AshEffect", "bot_0 ⇒ AshEffect");
    assert_eq!(fval(&msgs, 5, "pos_x").to_bits(), 5.0_f64.to_bits());
    assert_eq!(fval(&msgs, 5, "pos_y").to_bits(), 0.0_f64.to_bits());
    // 弹道穿过士兵但**不停**（士兵不挡弹道）
    assert!(!out.blocked);
    // 负样本：同一发换成人类皮肤就是 DeathEffect
    let mut q = ConvPredictor::new(
        &[],
        vec![
            Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 },
            Target { id: 7, x: 5.0, y: 0.0, radius: 0.5 },
        ],
        Mode::Normal,
    );
    q.skin.insert(7, "yellow".into());
    let (m2, _) = q.fire(1, "0.00", 0.0, 0.0, false, 100);
    assert_eq!(field(&m2, 5, "effect_type"), "DeathEffect");
}

// ---------------------------------------------------------------------------
// 6. 语料冒烟（没有语料就打印原因跳过，不静默通过）
// ---------------------------------------------------------------------------

/// 真语料里**已判定完全一致**的几份：逐字节比完整批窗口。
///
/// 这些名字与内容都来自只读语料，改动语料不该影响它们；一旦失败说明回归了。
#[test]
fn real_corpus_replays_that_are_byte_identical() {
    let dir = default_corpus_dir();
    if !dir.is_dir() {
        eprintln!("跳过：回放语料目录不存在（{}）", dir.display());
        return;
    }
    // 覆盖三条不同的机制：target ⇒ SparkEffect、多死效、DiffEq。
    let cases = ["1782640359", "1782640692", "1782760485"];
    let policy = strict_policy();
    for name in cases {
        let p = dir.join(name);
        if !p.is_file() {
            eprintln!("跳过 {name}：语料里没有这一份");
            continue;
        }
        let r = gw2_rebuild::diffconv::run_file(&p, &policy, 8);
        assert!(r.scored_batches > 0, "{name} 应该有结算批");
        assert!(r.clean, "{name} 应当完全一致，逐批成因：{:?}", r.causes);
    }
}
