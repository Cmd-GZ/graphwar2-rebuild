//! **「回放自身不自洽」的回归锁**（`dispatch/35`，`docs/06-verification.md` §44）。
//!
//! 这里的每一个数都是**回放报文里的原文与位模式**（`FunctionInfo.corrected_function`、
//! `EffectInfo(ExplosionEffect).pos_x/pos_y`），不是我们自己算出来再回头断言的。
//! 也就是说：这些断言钉的是**外部数据**，改实现不该让它们变色；它们一变就说明
//! `math_function` 的求值器或本判据的口径被动了。
//!
//! 口径（与 `examples/residual_fold.rs` / `tools/residual_fold.py` 同一条）：
//! `y_复算 = eval(parse(corrected_function), flip ? -pos_x : pos_x)`，
//! 与 `graphwar_shot_calculator::settle::explosion_point` 是**同一条式子**。
//!
//! 三组样本各说明一件事：
//!
//! | 组 | 断言 | 说明 |
//! |---|---|---|
//! | `SELF_CONSISTENT` | 复算 **等于** 回放 `pos_y` | 正对照：链路能得出「自洽」（否则"全都不自洽"只是工具坏了） |
//! | `OFF_BY_ONE_ULP` | 复算与回放 `pos_y` **恰好差 1 ulp**，且有符号 | 残差 `Y` 桶的形态（`docs/06` §44.3） |
//! | `MUZZLE` | `pos == (x0, y0)` 时 `pos_y` **不是**任何一次采样求值 | 数组退化到只剩起点 ⇒ 不能拿 `y=f(x)` 判它 |
use gw2_rebuild::math_function::{eval, parse};

/// `"0x401878d4fdf3b648"` → `u64`（回放里 `pos_y` 的位模式就是按这个形态记的）。
fn bits(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("位模式必须是 16 位十六进制")
}

fn hx(v: f64) -> String {
    format!("{:#018x}", v.to_bits())
}

/// `corrected_function` + `pos_x` + `flip` → 复算出来的 `pos_y`。
fn recompute(cf: &str, pos_x: f64, flip: bool) -> f64 {
    let x = if flip { -pos_x } else { pos_x };
    eval(&parse(cf), x, 0.0, 0.0)
}

/// 有符号 ulp 距离（`a` 相对 `b`）；0 = 逐位相同。
fn ulp(a: f64, b: f64) -> i64 {
    fn ord(x: f64) -> i64 {
        let w = x.to_bits() as i64;
        if w < 0 {
            i64::MIN - w
        } else {
            w
        }
    }
    ord(a).saturating_sub(ord(b))
}

/// **正对照**：内容命中的批（`replay_attrib` 的 `content_ok`）上，复算必须**逐位**命中。
///
/// 4 条各自来自一份不同的回放、不同的士兵、`flip` 两种都有。
const SELF_CONSISTENT: &[(&str, &str, bool, &str)] = &[
    // (回放 id，corrected_function，flip，pos_x)
    ("1782640836#0", "-11.54 + 17.658", true, "-24.999329797720257"),
    ("1782641987#0", "-0.2(x-4)^2 + 13.946", false, "4.354012878988948"),
    ("1782641987#3", "sin(x) - 6.101", true, "8.746977538378527"),
    ("1782641987#4", "7.8x/3 + 35.824", false, "-11.74772708524577"),
];

/// 对应上面 4 条的 `pos_y`（回放原文的位模式）。
const SELF_CONSISTENT_Y: &[&str] = &[
    "0x401878d4fdf3b648",
    "0x402bd784ca09eb4f",
    "0xc01ae98e1c676124",
    "0x40151ea09dd3972c",
];

#[test]
fn content_hit_batches_recompute_bit_exactly() {
    for (i, (id, cf, flip, px)) in SELF_CONSISTENT.iter().enumerate() {
        let v = recompute(cf, px.parse().unwrap(), *flip);
        assert_eq!(
            hx(v),
            SELF_CONSISTENT_Y[i],
            "{id}: 复算 {} != 回放 {} （文 {cf:?}）",
            hx(v),
            SELF_CONSISTENT_Y[i]
        );
    }
}

/// **残差 `Y` 桶的形态**：5 条全部**恰好差 1 ulp**，`+1` 与 `−1` 都有。
///
/// 这 5 条的 `(corrected_function, pos_x)` 与 `pos_y` 都是回放自己给的，
/// 而 `pos_x` 与回放的 `FunctionPoints.end_x` **逐位相同**（`docs/06` §44.5），
/// 所以这不是"取错了点"：是回放记录的 `y` 与它自己记录的式子差一个末位。
///
/// ⚠️ 真二进制自己的求值器（`0x4eb9d0`/`0x4ed670`，unicorn）在这 5 条上给出的
/// 值与 `recompute` **逐位相同**，所以这不是我方求值器的偏差（`tools/eval_oracle.py`）。
const OFF_BY_ONE_ULP: &[(&str, &str, bool, &str, &str, i64)] = &[
    // (回放 id，corrected_function，flip，pos_x，pos_y 位模式，有符号 ulp)
    ("1782641658#0", "(0.43*x) - 2.161", true, "-24.994024356021537", "0x40212c409d6e97a5", 1),
    ("1782642880#0", "sin(x6cos(5)) - 10.615", false, "-1.0476229930439582", "0xc0272f6480bfc54e", 1),
    ("1782642986#0", "19/(1+exp(999(x-8))) - 30.318", false, "-6.199671621782855", "0xc026a2d0e5604189", -1),
    ("1782646337#0", "-0.29(x)^2 + 10.691", true, "-6.322417849086799", "0xbfecd64ead4a355f", -1),
    ("1782646897#1", "1/(100(x-10)) + 6.282", false, "9.99952991831176", "0xc02dfb57096fa9b3", -1),
];

#[test]
fn y_bucket_is_exactly_one_ulp_off() {
    for (id, cf, flip, px, py, want) in OFF_BY_ONE_ULP {
        let v = recompute(cf, px.parse().unwrap(), *flip);
        let obs = f64::from_bits(bits(py));
        assert_ne!(v.to_bits(), obs.to_bits(), "{id}: 这一条本该对不上");
        assert_eq!(
            ulp(v, obs),
            *want,
            "{id}: 复算 {} vs 回放 {} 的有符号 ulp 差 {}，期望 {want}",
            hx(v),
            py,
            ulp(v, obs)
        );
    }
}

/// **炮口点**：`pos == (x0, y0)` 时 `pos_y` 是士兵站位，不是采样值 —— 必须落到「其它」。
///
/// 样本 `1782675099#1`（3 发批里的第 2 发）：`pos_x == x0`、`pos_y == y0`，
/// 而 `recompute(cf, pos_x)` 给出 `5.0e-25`（差 8.8e18 ulp）。这一条是
/// `examples/residual_fold.rs` 里 `pos_is_start` 那一档的回归锁：
/// 少了它，正对照里会凭空多出"不自洽"（`tools/residual_fold.py` 的正对照正是这么修的）。
#[test]
fn muzzle_point_is_not_a_sample() {
    let cf = "(8.440)*(1/((1+exp(-42*(x-(-19))))*(1+exp(42*(x-(25))))))\
+(15)*(1/((1+exp(-42*(x-(-19))))*(1+exp(42*(x-(0))))))\
+(13)*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(6))))))\
+(14)*(1/((1+exp(-42*(x-(9))))*(1+exp(42*(x-(10))))))\
+(f)*(1/((1+exp(-42*(x-(20))))*(1+exp(42*(x-(25)))))) - 8.440";
    let x0 = -20.407377496838247f64;
    let y0 = -8.439616121113485f64;
    let v = recompute(cf, x0, false);
    // 炮口点的 y 就是站位 y0（不是任何一次求值）
    assert_eq!(y0.to_bits(), bits("0xc020e1155d3df41e"), "y0 的位模式");
    assert!(
        ulp(v, y0).abs() > 1_000_000,
        "炮口点不该被当成一次采样：复算 {} vs y0 {}",
        hx(v),
        hx(y0)
    );
}
