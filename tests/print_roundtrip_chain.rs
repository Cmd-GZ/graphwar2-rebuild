//! **「打印-回代链」的回归锁**（`dispatch/56`，`docs/06-verification.md` §34）。
//!
//! 这条链是：`c`（f64）→ `{:.3}` 文本 → `parse` → `eval`。
//! 本文件锁四件事，全部用**回放报文里的原文与位模式**当输入，不自证：
//!
//! | 组 | 断言 | 说明 |
//! |---|---|---|
//! | [`printing_is_round_half_even_not_truncation`] | `{:.3}` 是**四舍五入**，不是截断 | 判别样本取自语料（两种规则给出不同的串） |
//! | [`replay_case_muzzle_is_the_start_point`] | 回放 `pos == (x0, y0)` 的那一发，`pos_y` **不是**任何回代值 | 弹道退化到只剩起点（`5/155sin(4exp(4*6cos(`） |
//! | [`replay_case_is_one_ulp_out_of_reach`] | 回代的 1 ulp 差**钉在回放数据上**，不是我们没实现某条规则 | `19/(1+exp(999(x-8))) - 30.318` @ `x=-6.199671621782855` |
//! | [`prefix_add_agrees_with_postfix_sub`] | 我方"前置 `Add(c, tree)`"与真值"后置 `Sub/Add(tree, \|c\|)` 数值逐位相同 | `§26.5` ① 的数值侧；token 序列**不同**是已知的 |
//!
//! 这些断言钉的是**外部数据**（回放里的文本与位模式）：改实现不该让它们变色。
//! 它们一变，说明 `correction_text` 的打印规则、`math_function` 的求值器、
//! 或"修正常数如何入树"被动了。
use gw2_rebuild::graphwar_shot_calculator::{correction_text, shifted_tokens};
use gw2_rebuild::math_function::{eval, parse};

fn hx(v: f64) -> String {
    format!("{:#018x}", v.to_bits())
}

/// 回代：`eval(parse(corrected_function), flip ? -x : x)`。
fn recompute(cf: &str, pos_x: f64, flip: bool) -> f64 {
    let x = if flip { -pos_x } else { pos_x };
    eval(&parse(cf), x, 0.0, 0.0)
}

/// **打印格式 = 四舍五入（半值取偶），不是向零截断。**
///
/// 空函数（`function == ""`）时 `c = y0 − f(∓x0) = y0 − 0 = y0`，所以这两个 `y0`
/// 就是常数本身；两个样本都取自语料，且**两种规则给出不同的串**（否则测不出东西）：
///
/// | `y0`（回放 `FunctionPoints.start_y` 的位模式） | 四舍五入 | 截断 |
/// |---|---|---|
/// | `3.4458198403310867` | ` + 3.446` | ` + 3.445` |
/// | `-30.318323909771305` | ` - 30.318` | ` - 30.318`（这一条不判别，留作符号回归） |
///
/// 语料级的分母在 `docs/06` §34：Y+X 的 186 发 **186 / 186** 与四舍五入一致，
/// 而截断只对 107 发；647 个命中批的 367 发是 **367 / 367** 与截断 182 发。
#[test]
fn printing_is_round_half_even_not_truncation() {
    // c = 3.4458198403310867（1782642986#0 #1 那一发的 y0）
    let toks = parse("");
    assert_eq!(correction_text(&toks, 0.0, 3.4458198403310867, false), " + 3.446");
    // 截断会给出 " + 3.445" —— 语料里一发都没有
    assert_ne!(correction_text(&toks, 0.0, 3.4458198403310867, false), " + 3.445");

    // 负常数：符号在外、幅值打印（真值 `0x3c604d` 的 `andpd` = 清符号位）
    assert_eq!(correction_text(&toks, 0.0, -30.318323909771305, false), " - 30.318");
}

/// **`pos == (x0, y0)` 的那一发：爆炸点就是起点，回代不适用。**
///
/// `1782642986` 第 1 批第 3 发的原文是 `5/155sin(4exp(4*6cos(`（括号缺 1 个），
/// 回放的 `EffectInfo(ExplosionEffect)` 与 `FunctionPoints.start` **逐位相同**：
/// 弹道退化成"只有一个起点"，采样器一步都没走成。
/// 所以拿 `y = f(x)` 去判这一发会得到 `-0.0174…`（离 `6.05` 差得远）—— 那不是错，
/// 是**判据不适用**。
#[test]
fn replay_case_muzzle_is_the_start_point() {
    let x0 = -2.632134919326489_f64;
    let y0 = 6.05141758173637_f64;
    // 回放里这一发的 pos
    assert_eq!(hx(x0), "0xc0050e9cc0a955bf");
    assert_eq!(hx(y0), "0x401834a6cf7ffd0c");
    // 回代在起点的值不是 pos_y（差得远）⇒ 这一发的 pos_y 不能按"回代"判
    let cf = "5/155sin(4exp(4*6cos())) + 6.055";
    let got = recompute(cf, x0, false);
    assert_ne!(hx(got), hx(y0));
    assert!((got - y0).abs() > 1e-3, "回代在退化弹道上本来就不等于 pos_y");
}

/// **Y 桶那 1 ulp：回放的 `pos_y` 用回放自己的 `(corrected_function, x)` 复现不出来。**
///
/// 样本 `1782642986` 第 1 批第 1 发（`Normal`）：
///
/// ```text
/// function          = 19/(1+exp(999(x-8)))
/// corrected_function= 19/(1+exp(999(x-8))) - 30.318
/// start             = (-16.015296621781708, -11.318323909771305)
/// c（全精度）        = -30.318323909771305      （= y0 - f(-x0)，f(-x0)=19）
/// 回放 pos           = (-6.199671621782855, -11.318)      ← pos_x 与 end_x 逐位相同
/// 回代（3 位小数常数）= -11.318000000000001      = 19 - fl(30.318)
/// 全精度常数          = -11.318323909771305      （差 5e-4，量级就差两条街）
/// ```
///
/// 关键在这一步：`19 − fl(30.318)` 是**精确**的（两个数都是 `2^-48` 的整数倍，
/// 结果落在 `[8,16)` 的网格上）⇒ 它是唯一可能的 f64 结果，而回放记的是
/// `fl(-11.318)`（= 十进制 `-11.318` 的最近 f64），比它大 1 ulp。
/// **任何 f64 的 `x` 都得不到 `fl(-11.318)`**：`x` 必须在
/// `[−20.9114607451334811, −20.9114607451334793]` 之间，而这个区间里没有可表示的 f64
/// （详见 `docs/06` §34.4）。
///
/// ⇒ 这不支持"打印规则不对"，而是"回放那个 `pos_y` 不是 f64 回代能产出的值"。
#[test]
fn replay_case_is_one_ulp_out_of_reach() {
    let x0 = -16.015296621781708_f64;
    let y0 = -11.318323909771305_f64;
    let pos_x = -6.199671621782855_f64;
    let obs_y = -11.318_f64;

    // ① 常数：全精度 → 3 位小数（这一步语料里 186/186 一致）
    let toks = parse("19/(1+exp(999(x-8)))");
    let c = gw2_rebuild::graphwar_shot_calculator::correction(&toks, x0, y0, false);
    assert_eq!(hx(c), "0xc03e517dacfe00ab", "c = y0 - f(∓x0)");
    assert_eq!(correction_text(&toks, x0, y0, false), " - 30.318");

    // ② 回代：回放自己的 corrected_function @ 回放自己的 pos_x
    let cf = "19/(1+exp(999(x-8))) - 30.318";
    let got = recompute(cf, pos_x, false);
    assert_eq!(hx(got), "0xc026a2d0e560418a", "19 - fl(30.318) 是精确值");
    assert_eq!(hx(obs_y), "0xc026a2d0e5604189", "回放记的是 fl(-11.318)");
    assert_eq!(
        got.to_bits().abs_diff(obs_y.to_bits()),
        1,
        "差恰好 1 ulp（相邻两个 f64）"
    );

    // ③ "换全精度常数"不是答案：那样差 5e-4，不是 1 ulp
    let full = eval(
        &shifted_tokens(&toks, c),
        pos_x,
        0.0,
        0.0,
    );
    assert!((full - obs_y).abs() > 1e-4, "全精度常数差 5e-4 量级");
    assert_ne!(hx(full), hx(c));
}

/// **正对照：真的能命中。**（同一套复算，换一条回放里就逐位相等。）
///
/// `1782642582` 第 3 批第 1 发（`Normal`）：
/// `corrected_function = +6/(1+exp(999(x+15)))-10/(1+exp(999(x-17.5))) + 7.333`，
/// 回放 `pos = (14.751830428335264, -2.667)`。
#[test]
fn positive_control_recomputes_exactly() {
    let cf = "+6/(1+exp(999(x+15)))-10/(1+exp(999(x-17.5))) + 7.333";
    let got = recompute(cf, 14.751830428335264, false);
    assert_eq!(hx(got), "0xc0055604189374bc", "= -2.667，与回放 pos_y 逐位相同");
}

/// **前置 `Add(c, tree)` ≡ 后置 `Sub/Add(tree, |c|)`**（数值上；token 序列不同，`§26.5` ①）。
///
/// 采样器内部我方用前置写法（[`shifted_tokens`]），真机是后置（`[..., Sub, |c|]`）。
/// IEEE 加法可交换、`Sub(tree,|c|) ≡ Add(tree,-|c|)`，所以**数值逐位相同**是恒等式，
/// 不是巧合；这条测试只是把它钉住，防止哪天有人改成"先乘后加"之类。
#[test]
fn prefix_add_agrees_with_postfix_sub() {
    let cases = [
        ("(-6.474)*(1/((1+exp(-42*(x-(-10))))*(1+exp(42*(x-(25))))))", 22.0_f64, 11.952342932277856_f64),
        ("19/(1+exp(999(x-8)))", -6.0_f64, -11.318_f64),
        ("sin(4x)+x^2/3", 1.5_f64, 2.75_f64),
    ];
    for (text, x, c) in cases {
        let toks = parse(text);
        let pre = eval(&shifted_tokens(&toks, c), 0.75, 0.0, 0.0);
        let post = eval(&parse(&format!("{text} + {c:?}")), 0.75, 0.0, 0.0);
        assert_eq!(pre.to_bits(), post.to_bits(), "前置/后置在 {text} 上必须逐位相同");
        let post_neg = eval(&parse(&format!("{text} - {:?}", -c)), 0.75, 0.0, 0.0);
        assert_eq!(pre.to_bits(), post_neg.to_bits(), "Sub 与 Add 负常数同义");
        let _ = x;
    }
}
