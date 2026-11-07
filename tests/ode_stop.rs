//! `Mode::Ode` 的**停止判据**验收（`dispatch/29`，`docs/06-verification.md` §39）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：这些断言是**回归保护**，不是复原证据。
//! 复原证据是下面这套地址上的机器码，本文件只是把结论钉住、防止以后被"顺手改回去"。
//!
//! # 原版停止判据（**观测：逐指令**，两条架构互证）
//!
//! 采样器 = `GraphwarShotCalculator::calculate_function`
//! （arm64 未 strip，符号在 `.dynsym`：`0x3b5374`；x86 同函数 `0x3c2530`）。
//! Ode 分支：x86 `0x3c25f8` 的 `je 0x3c2923` **不跳**（mode != 0）即进入 `0x3c25fe`。
//!
//! | 出口 | x86 | 判据 |
//! |---|---|---|
//! | 二分 100 次不收敛 | `0x3c2677` `dec ebx` / `0x3c282f` `jne` / `0x3c2835` | 无 |
//! | 候选点 NaN | `0x3c287d` `ucomisd cand_y,px` + `0x3c2881` `jp` | `cand_y` 或 `px` 是 NaN |
//! | x 越界 | `0x3c2890` `andpd` / `0x3c2898` `ucomisd [0x19fbc8]` / `0x3c28a0` `ja` | `\|mid\| > 25` |
//! | y 越界 | `0x3c28b2` `andpd` / `0x3c28b4` `ucomisd [0x19fd40]` / `0x3c28bc` `ja` | `\|cand_y\| > 15` |
//! | 点数用尽 | `0x3c2915` `cmp r13d,r12d` / `0x3c2918` `jne` | `count == (200/step)` |
//!
//! 常量（`.rodata`，逐字节读出）：`0x19fbc8 = 25.0`、`0x19fd40 = 15.0`、
//! `0x19fc78 = 0.001`（弦长容差）、`0x19f8c0 = 200.0`（总长）、
//! `0x19fbc0/0x19f7e0 = ±2^31`（`as i32` 的饱和夹）、`0x19f098 = 6.0`、`0x19f6d0 = 0.5`。
//! 预算算法 `0x3c25bd..0x3c25f1`：`n = max(1, saturating_i32(200.0/step))`。
//! 五个出口全部在**收敛之后**才判（x86 `0x3c27d0` 的 `ja 0x3c2840` 之前不查边界），
//! `src/graphwar_path_calculator.rs` 逐条同构 —— 本文件的值就是这套判据的行为面。
//!
//! 复跑（两条都要过）：
//! ```text
//! cargo test --offline --test ode_stop
//! cargo run --release --offline -q --example ode_stop_trace -- \
//!     --text "" --x0 20 --y0 0 --ode --tail 2
//! ```

use gw2_rebuild::graphwar_path_calculator::{calculate, point_budget, Mode};
use gw2_rebuild::math_function::parse;
use gw2_rebuild::{MAX_ABS_X, MAX_ABS_Y, SERVER_STEP};

fn run(text: &str, x0: f64, y0: f64, flip: bool, mode: Mode) -> Vec<(f64, f64)> {
    calculate(&parse(text), x0, y0, SERVER_STEP, flip, mode)
}

// ---------------------------------------------------------------------------
// 1. 点数预算：`max(1, saturating_i32(200.0 / step))`（`0x3c25bd..0x3c25f1`）
// ---------------------------------------------------------------------------

#[test]
fn point_budget_matches_the_binary_formula() {
    // 服务端步长 0.01 ⇒ 20000（`0x19f8c0 / step`）
    assert_eq!(point_budget(SERVER_STEP), 20000);
    // 只有 2 个点：分母小的时候走 `cvttsd2si`
    assert_eq!(point_budget(100.0), 2);
    // 小于 2 ⇒ 夹到 1（`0x3c25e8 cmp $0x2` + `cmovge`）
    assert_eq!(point_budget(1.0e9), 1);
    // NaN ⇒ 0 ⇒ 再夹到 1（`0x3c25e1 cvttsd2si` + `0x3c25e5 cmovp`）
    assert_eq!(point_budget(f64::NAN), 1);
    // +∞ ⇒ 饱和到 i32::MAX（`maxsd/minsd` 夹 ±2^31）
    assert_eq!(point_budget(0.0), i32::MAX);
}

// ---------------------------------------------------------------------------
// 2. x 越界：`|mid| > 25`（`0x3c28a0`）
// ---------------------------------------------------------------------------

#[test]
fn ode_stops_at_the_x_bound_25() {
    // 空函数树 ⇒ `dy/dx = 0` ⇒ 一条 y = y0 的直线（原版的空函数就是这条直线，
    // 证据见 `docs/06` §39.4：回放 1782764906 批 #0/#1/#5 的 end_x 落在本函数的点列上）
    let pts = run("", 20.0, 0.0, false, Mode::Ode);
    assert!(pts.len() > 100, "应当一路采到边界，实际只有 {} 点", pts.len());
    assert_eq!(pts[0], (20.0, 0.0), "首点是开火坐标，必须原样入列");
    for (x, y) in &pts {
        assert!(x.abs() <= MAX_ABS_X, "入列点必须满足 |x| <= 25，实际 x={x}");
        assert_eq!(*y, 0.0, "dy/dx = 0 ⇒ y 全程不动");
    }
    let last = pts.last().copied().unwrap();
    assert!(
        last.0.abs() > MAX_ABS_X - SERVER_STEP,
        "停点应贴着 25 的边界（还差 {:e}）",
        MAX_ABS_X - last.0.abs()
    );
}

// ---------------------------------------------------------------------------
// 3. y 越界：`|cand_y| > 15`（`0x3c28bc`）
// ---------------------------------------------------------------------------

#[test]
fn ode_stops_at_the_y_bound_15() {
    // dy/dx = 50 ⇒ 从 (0,0) 出发每 0.01 的 x 走 ~0.5 的 y，很快撞 15
    let pts = run("50", 0.0, 0.0, false, Mode::Ode);
    assert!(pts.len() > 1, "应当先接受若干点再撞 y 边界");
    for (_, y) in &pts {
        assert!(y.abs() <= MAX_ABS_Y, "入列点必须满足 |y| <= 15，实际 y={y}");
    }
    let last = pts.last().copied().unwrap();
    assert!(
        last.1 > MAX_ABS_Y - 1.0,
        "停点应贴到 15（实际 y={}）",
        last.1
    );
    assert!(last.0.abs() < 1.0, "撞 y 边界时 x 还没走远（实际 x={}）", last.0);
}

// ---------------------------------------------------------------------------
// 4. NaN：`0x3c2881 jp`（比较 `cand_y` 与 `px` 的次序，任一 NaN 即停）
// ---------------------------------------------------------------------------

#[test]
fn ode_stops_when_the_candidate_is_nan() {
    // sqrt(-1) 恒为 NaN；`px` 不是 NaN，所以走的是"cand_y 是 NaN"这一支
    let pts = run("sqrt(0-1)", 0.0, 0.0, false, Mode::Ode);
    assert_eq!(pts.len(), 1, "候选点 NaN ⇒ 一条点都不入列，只留首点");
    assert_eq!(pts[0], (0.0, 0.0));
}

// ---------------------------------------------------------------------------
// 5. 两个模式共用同一套出口，但**采样路径不同**（`0x3c25f8 je 0x3c2923`）
// ---------------------------------------------------------------------------

#[test]
fn ode_and_normal_are_different_samplers() {
    // 同一个串 `x`，同一个起点 (0,0)：
    // * Normal 把它当 `y = f(x)` ⇒ 点列贴着 y = x 走；
    // * Ode 把它当 `dy/dx = f(x, y)` ⇒ y ≈ x²/2，很快撞到 y 的边界。
    // 两条必须**从第一个接受点起就不同**、终点也不同 —— 若相同说明 mode 没被用上。
    let n = run("x", 0.0, 0.0, false, Mode::Normal);
    let o = run("x", 0.0, 0.0, false, Mode::Ode);
    assert!(n.len() > 2 && o.len() > 2, "两条都要真的走起来");
    assert_ne!(n[1], o[1], "第一个接受点就应不同：Normal {:?} / Ode {:?}", n[1], o[1]);
    assert_ne!(n.last().copied(), o.last().copied(), "停点不应相同");
    assert!(
        o.len() < n.len(),
        "Ode 的 `x` 会在 y 方向加速、先撞 15：Normal {} / Ode {}",
        n.len(),
        o.len()
    );
}

// ---------------------------------------------------------------------------
// 6. flip：只镜像 x（`0x3c25b5 xorpd [0x19e690]`），不改 y、不改走了多远
// ---------------------------------------------------------------------------

#[test]
fn flip_mirrors_the_whole_trajectory() {
    // 从 +20 朝 +x 采（flip=false）与从 −20 朝 −x 采（flip=true）是**镜像**的两条，
    // 点列逐点相反 —— 这条同时验了"首点原样入列、后续 px 取反"（`0x3c2588` / `0x3c2816`）。
    let a = run("", 20.0, 1.5, false, Mode::Ode);
    let b = run("", -20.0, 1.5, true, Mode::Ode);
    assert_eq!(a.len(), b.len(), "flip 不该改变点数");
    for (p, q) in a.iter().zip(b.iter()) {
        assert_eq!(q.0, -p.0, "flip 把 x 取反");
        assert_eq!(p.1, q.1, "flip 不动 y");
    }
    assert!(a.last().unwrap().0 > 24.0 && b.last().unwrap().0 < -24.0);
}
