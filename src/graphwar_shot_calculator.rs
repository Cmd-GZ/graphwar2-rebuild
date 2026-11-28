//! 开火结算里"让曲线穿过士兵"的那一步。
//!
//! 对应二进制：修正常数 `@0x3c5f70`，改写函数串 `@0x3c5c50`。
//! 见 `docs/05-shot-pipeline.md`。

use crate::math_function::{eval, parse};
use crate::graphwar_path_calculator::{calculate, Mode};
use crate::{Function, Token, STEP_DEFAULT};

/// 开火结算的"停止点 + 致死"那一半（原版同一个文件，我方按主题拆出）。
pub mod settle;

/// 修正常数 `c = y0 - f(±x0)`；实际射击使用 `f(x) + c`。
pub fn correction(tokens: &[Token], x0: f64, y0: f64, flip: bool) -> f64 {
    y0 - eval(tokens, if flip { -x0 } else { x0 }, 0.0, 0.0)
}

/// 右下角那行字：`" + 0.591"` / `" - 1.000"` / `" + NaN"`（3 位小数）。
pub fn correction_text(tokens: &[Token], x0: f64, y0: f64, flip: bool) -> String {
    let c = correction(tokens, x0, y0, flip);
    if c.is_nan() {
        // 原程序按 `c < 0.0` 分支，NaN 走 '+' 那一侧
        return " + NaN".to_string();
    }
    if c >= 0.0 {
        format!(" + {:.3}", c)
    } else {
        format!(" - {:.3}", -c)
    }
}

/// 把 `f(x)+c` 表示成前缀 token 列表。
pub fn shifted_tokens(tokens: &[Token], c: f64) -> Vec<Token> {
    let mut out = vec![Token::op(Function::Add), Token::constant(c)];
    out.extend_from_slice(tokens);
    out
}

/// 端到端：一次射击产生的点序列。
pub fn shot_points(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    apply_correction: bool,
) -> Vec<(f64, f64)> {
    let mut toks = parse(text);
    if toks.is_empty() {
        // 空函数文本（在线回放里真的出现过：`function == ""`）在 **Normal** 局没有弹道可言，
        // 保持早退。但 `Mode::Ode` 局里"导数恒 0"是合法语义（水平直线），空 token 树交给
        // `calculate` 就是这条直线 —— 2026-11-08 任务 29 的普查里 4 批（`1782764906` #0/#1/#5、
        // `1783342151` #2）正是这样，早退会让整条弹道消失（`end_x = 0.0`）。
        // 闸门写成 `mode != Mode::Ode` ⇒ Normal 的早退逐字保留（实测 Normal 355/331 一条不动）。
        // 档位：**推断** —— "空 = 导数 0"是从这 4 批的改善反推的，不是从二进制读出来的。
        if mode != Mode::Ode {
            return Vec::new();
        }
    }
    if apply_correction && mode == Mode::Normal {
        let c = correction(&toks, x0, y0, flip);
        toks = shifted_tokens(&toks, c);
    }
    calculate(&toks, x0, y0, step, flip, mode)
}

/// 便捷封装：服务端步长 + 普通模式。
pub fn shot_default(text: &str, x0: f64, y0: f64) -> Vec<(f64, f64)> {
    shot_points(text, x0, y0, STEP_DEFAULT, false, Mode::Normal, true)
}

// ---------------------------------------------------------------------------
// 开火模拟：把三块已验证的积木拼起来
// ---------------------------------------------------------------------------
//
// 组成：掩膜（创建 + 光栅化障碍）→ 点序列（采点）→ 逐点查掩膜。
//
// 三块积木各自都已与真二进制对拍：
//   * 采点      `src/graphwar_path_calculator.rs`     （331 条真值夹具）
//   * 光栅化    `src/plane.rs`    （逐字节一致）
//   * 点测试    `src/plane.rs`    （300 随机点一致）
//
// **但"拼装方式"本身没有在二进制里找到对应的函数**（真正的开火循环还没定位），
// 所以这里只是把已验证的积木按最自然的顺序组合，并把它当作**可验证的原语**，
// 而不是"游戏就这么做的"结论。

use crate::plane::{empty_mask, is_occupied, mask_index, rasterize, CANVAS_H, CANVAS_W};

/// 障碍物：游戏坐标中心 + 半径（单位与 `size` 相同）。
#[derive(Debug, Clone, Copy)]
pub struct Obstacle {
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

/// 开火模拟结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotOutcome {
    /// 命中的点序号（`0` = 起点）；未命中为 `None`。
    pub hit_index: Option<usize>,
    /// 点序列长度。
    pub point_count: usize,
}

/// 跑一次模拟：先按障碍建掩膜，再沿点序列逐点查。
///
/// 起点本身通常落在开火士兵身上；这里**不**做"忽略起点"的特殊处理——
/// 掩膜里只有调用方给的障碍，所以起点是否命中取决于障碍怎么给。
pub fn simulate(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    obstacles: &[Obstacle],
) -> ShotOutcome {
    let mut mask = empty_mask();
    for ob in obstacles {
        rasterize(&mut mask, ob.x, ob.y, ob.size, 0x00);
    }
    let pts = shot_points(text, x0, y0, step, flip, mode, true);
    let hit_index = pts.iter().position(|&(x, y)| is_occupied(&mask, x, y));
    ShotOutcome { hit_index, point_count: pts.len() }
}

/// 掩膜尺寸，供外部校验缓冲区。
pub const MASK_BYTES: usize = CANVAS_W as usize * CANVAS_H as usize;

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // 真值来源（本模块三条 shot 测试共用）
    // -----------------------------------------------------------------------
    //
    // **逐位**：`unicorn` 直调真二进制三个函数串起来 ——
    //   ① 求值器 `0x4eb9d0` 算修正常数 `c = y0 - f(x0)`；
    //   ② 采样器 `0x3c2530` 出整条点数组（`edx=flip`、`ecx=mode`）；
    //   ③ 光栅化 `0x3f11e0` 出真掩膜、单点碰撞 `0x3f15b0` 逐点查。
    // 复算：`.reference/venv/bin/python tools/gen_test_expectations.py`
    //   → `fixtures/test_expectation_truth.tsv` 的 `shot/B01..B03` 段。
    // ⚠️ 只有**组合方式**（把三个原语按"建掩膜→沿点查"拼起来）是我方推断，
    //    二进制里没找到对应的开火循环（见本文件 §"拼装方式"的说明）。

    /// `sin(x)` + 修正、从 (5,2)、step=0.01、不反向：真二进制给 **2471** 点，
    /// 末点 = `(4038ff851eb85168, 40069903c94a8282)`；整条点数组 FNV 另记在夹具表里。
    #[test]
    fn no_obstacles_means_no_hit() {
        let r = simulate("sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal, &[]);
        assert_eq!(r.hit_index, None);
        // ★ 改前只有 `point_count > 100`（近乎恒真）；现在钉真二进制的点数
        assert_eq!(r.point_count, 2471, "点数与真采样器 0x3c2530 不一致");
        // 点值也钉住（真采样器逐位输出）
        let pts = shot_points("sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal, true);
        let b = |v: f64| v.to_bits();
        assert_eq!(pts.len(), 2471);
        assert_eq!((b(pts[0].0), b(pts[0].1)), (0x4014_0000_0000_0000, 0x4000_0000_0000_0000));
        assert_eq!((b(pts[1].0), b(pts[1].1)), (0x4014_08f5_c28f_5c29, 0x4000_0528_8903_d7b2));
        assert_eq!((b(pts[100].0), b(pts[100].1)), (0x4017_4000_0000_0012, 0x4004_0b1d_5cb3_4366));
        assert_eq!((b(pts[300].0), b(pts[300].1)), (0x401d_6000_0000_004b, 0x400e_a706_aa23_0e22));
        assert_eq!((b(pts[2470].0), b(pts[2470].1)), (0x4038_ff85_1eb8_5168, 0x4006_9903_c94a_8282));
    }

    /// 曲线上放障碍 ⇒ 命中。
    ///
    /// 真值（`unicorn`）：`c` 位 = `4007abe07d78264d`；障碍中心 `ob.y` 位 = `400f961525825a2c`；
    /// 真掩膜 + 真点数组 + 真单点碰撞 ⇒ **首个命中点序号 341**，
    /// 该点坐标 = `(401ed5c28f5c2946, 400f965100411e3c)`。
    #[test]
    fn obstacle_on_the_curve_is_hit() {
        // sin(x) 在 x=5 附近 y≈-0.959 + 修正 2.9589 ≈ 2.0（起点在曲线上）
        let curve_y = (5.0f64).sin() * 1.0;
        let c = 2.0 - curve_y;
        let ob = Obstacle { x: 8.0, y: (8.0f64).sin() + c, size: 0.3 };
        assert_eq!(ob.y.to_bits(), 0x400f_9615_2582_5a2c, "障碍中心与真求值器不一致");
        let r = simulate(
            "sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal,
            &[ob],
        );
        assert!(r.hit_index.is_some(), "曲线上放了障碍却没命中");
        assert!(r.hit_index.unwrap() > 1, "不应在起点就命中");
        // ★ 改前只有 `is_some()` / `> 1`；现在钉真二进制判出来的序号与坐标
        assert_eq!(r.hit_index, Some(341), "首个命中点序号与真二进制不一致");
        let pts = shot_points("sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal, true);
        assert_eq!(
            (pts[341].0.to_bits(), pts[341].1.to_bits()),
            (0x401e_d5c2_8f5c_2946, 0x400f_9651_0041_1e3c),
            "命中点坐标与真采样器不一致"
        );
    }

    /// 障碍远离曲线 ⇒ 不命中（真二进制：整条 2471 点里一个都不撞）。
    ///
    /// 改前这条只有 `hit_index == None`：**实现退化成"弹道只剩 1 点"时也会绿**
    /// （`AGENTS.md` §0.5 引的那次事故就是这种退化）。现在同时钉点数。
    #[test]
    fn obstacle_away_from_the_curve_is_missed() {
        let r = simulate(
            "sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal,
            &[Obstacle { x: 8.0, y: -12.0, size: 0.3 }],
        );
        assert_eq!(r.hit_index, None, "障碍远离曲线却命中了");
        // ★ 真采样器：这条弹道是 2471 点（不是 1 点、也不是 0 点）
        assert_eq!(r.point_count, 2471, "点数与真采样器 0x3c2530 不一致");
    }

    #[test]
    fn mask_bytes_matches_canvas() {
        assert_eq!(MASK_BYTES, 1000 * 600);
        assert_eq!(mask_index(0, 0), 0);
        assert_eq!(mask_index(999, 599), 1000 * 600 - 1);
    }
}

/// 与游戏一致的"修正后函数"文本：`原函数` + **带符号**的修正常数（三位小数）。
///
/// 依据是真实抓包（`docs/24` §6.2；`docs/127` §2 的原始字节流帧号）：
///
/// ```text
/// {"FunctionInfo":{"owner_id":234,"function":"0.00","corrected_function":"0.00 + 4.638"}}
/// {"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" + 10.990"}}
/// {"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" - 3.686"}}             ← 帧 182（c < 0）
/// {"FunctionInfo":{"owner_id":233,"function":"abs((abs((-0.57+x))+-4.85))",
///                  "corrected_function":"abs((abs((-0.57+x))+-4.85)) - 11.416"}}               ← 帧 203（c < 0）
/// ```
///
/// ⇒ **运算符本身带符号**：`c >= 0` 写 `" + "`、`c < 0` 写 `" - "`，
/// 而**不是** `" + -3.686"`。这与右下角那行字（[`correction_text`]）是同一个格式，
/// 所以直接复用，不另写一套。
///
/// ⚠️ 2026-11 之前这里是写死的 `format!("{} + {:.3}", …)`：`c < 0` 时会发出
/// `" + -3.686"`，真机从没这么写过（`docs/127` §5 第 1 条、§2.1 第 3 条）。
pub fn corrected_text(text: &str, x0: f64, y0: f64, flip: bool) -> String {
    GraphwarShotCalculator::calculate_corrected_function(text, x0, y0, flip)
}

// ---------------------------------------------------------------------------
// 原版 `GraphwarShotCalculator` 的三条关联函数（**真名**，arm64 `.symtab` 原文）
// ---------------------------------------------------------------------------
//
// 这是 T108（`docs/06` §156）把「527 分母之外真缺」的 3 条落成源码级函数边界的产物。
// 判据/输入集 = 预注册件 `fixtures/denom_drawers_math_plan.txt`；逐条台账 =
// `fixtures/denom_drawers_math_anchors.tsv`。
//
// | 原版函数 | Android 2.0.60 arm64 `.symtab`（未 strip） | Linux x86-64 `.eh_frame`（`addr_probe fde` 现取） |
// |---|---|---|
// | `calculate_function` | `0x3b5374`, 1020 B | `0x3c2530`..`0x3c2bb9`（0x689） |
// | `calculate_corrected_function` | `0x3b79a8`, 712 B | `0x3c5c50`..`0x3c5f68`（0x318） |
// | `calculate_function_offset_str` | `0x3b7c70`, 744 B | `0x3c5f70`..`0x3c62a7`（0x337） |
//
// 跨构建配对的三条独立链（**观测**，判据见预注册 §2.1）：
//  1. **相邻性**：Android `0x3b5374 + 1020 = 0x3b5770` = `calculate_shot_results` 的起始；
//     Linux `0x3c2530 + 0x689` 补齐到 `0x3c2bc0` = `calculate_shot_results`（T81 隔离对拍用过）。
//     `calculate_corrected_function` 后面紧接 `calculate_function_offset_str`，两份构建都严丝合缝。
//  2. **值级对拍**：三条在 unicorn 下跑 Linux **真机器码**，输出与本文实现逐位/逐字节相同
//     （`fixtures/denom_drawers_math_truth.tsv`；复算 `tools/denom_drawers_truth.py`）。
//  3. `panic::Location` 对本批**用不上**：三条体内 gameLOC = 0（T105 观测）—— 这正是它们落在
//     527 分母**之外**的原因，**不是**"它们不存在"。
//
// ABI（三条函数的序言）：`rdi` 全是 sret、**没有 self** ⇒ 关联函数（**推断**：Rust ABI 下
// `&self` 必然占一个寄存器，而三条的 rdi 都被 sret 占了）。
//   `calculate_function`:            rdi=sret(Vec<Vec2>), rsi=&Vec<Token>, edx=flip, ecx=mode,
//                                    xmm0/1/2 = x0, y0, step
//   `calculate_corrected_function`:   rdi=sret(String), rsi=文本指针, rdx=文本长度,
//                                    rcx=`&Point{x,y}`（16 B）, r8b=flip
//   `calculate_function_offset_str`:  同上
//
// ⚠️ **为什么不把本体搬进来**（预注册 §2.5 K1）：`calculate_function` 的本体是
// `src/graphwar_path_calculator.rs::calculate` —— 那个文件不在本任务的写范围，且采样器
// `0x3c2530` 的**模块归属**按本文的相邻性证据其实属于 `graphwar_shot_calculator`（`docs/02` §20.5
// 记的是"文档级归属"）。搬迁登记为遗留（`docs/06` §156.6），不在这条 commit 里做。
pub struct GraphwarShotCalculator;

impl GraphwarShotCalculator {
    /// `GraphwarShotCalculator::calculate_function`（Linux **`0x3c2530`** / arm64 `0x3b5374`）
    ///
    /// 弹道点序列采样器（等弦长二分追赶），返回 `Vec<{x, y}>`（每点 16 B）。
    ///
    /// 指令锚（x86-64 Linux）：
    ///  * `0x3c2541 mov r15d,ecx` / `0x3c254a mov ebp,edx` ⇒ 第 3/4 参都是**一个字节**（bool + mode）
    ///  * `0x3c255d`..`0x3c259b` 先给 `Vec` 容量 8，再把 `(x0, y0)` 写成首点
    ///  * `0x3c25a4 test bpl,bpl` + `0x3c25b5 xorpd xmm1,[0x19e690]` ⇒ `flip` 时把 `x0` 取反
    ///  * 末尾 `0x3c257d call *[0x9baca0] -> 0x460c20` 是 `Vec` 扩容 helper（另一处 `0x3c2…`）
    pub fn calculate_function(
        tokens: &[Token],
        flip: bool,
        mode: Mode,
        x0: f64,
        y0: f64,
        step: f64,
    ) -> Vec<(f64, f64)> {
        calculate(tokens, x0, y0, step, flip, mode)
    }

    /// `GraphwarShotCalculator::calculate_function_offset_str`（Linux **`0x3c5f70`** / arm64 `0x3b7c70`）
    ///
    /// 右下角那行修正量的文本：`" + 0.591"` / `" - 1.000"` / `" + NaN"` / `" + -0.000"`
    /// （前缀 **3 字节** `" + "` 或 `" - "` + `{:.3}` 格式化的数）。
    ///
    /// 指令锚（x86-64 Linux）：
    ///  * `0x3c5ffd call *[0x9bacf8] -> 0x4ed670` ⇒ 先用 `MathFunction::new` 解析文本
    ///  * `0x3c6003 movsd xmm0,[r14]` + `0x3c600d xorpd xmm0,[0x19e690]` ⇒ `flip` 时 `Point.x` 取反
    ///  * `0x3c602a call 0x4eb9d0` ⇒ 求值器算 `f(±x0)`（`xmm1=y=0`、`xmm2=dy=0`）
    ///  * `0x3c602f movsd xmm1,[r14+8]` + `0x3c6035 subsd xmm1,xmm0` ⇒ **`c = y0 - f(±x0)`**
    ///  * `0x3c6043 ucomisd xmm0,xmm1` + `0x3c6047 jbe 0x3c6100` ⇒ **`0.0 <= c` 走 `+`**
    ///    （NaN 时 `ucomisd` 置 CF=ZF=1，`jbe` 也走 `+` —— 所以 NaN 打印 `" + NaN"`）
    ///  * `0x3c604d andpd xmm1,[0x19c050]` ⇒ 负数分支取绝对值（`0x19c050` = 符号位掩膜 `7fff…ffff`）
    ///  * `0x3c60b8 mov byte [rax+2],0x20` + `0x3c60bc mov word [rax],0x2d20` ⇒ 分配 3 字节前缀
    ///    `" - "`；`+` 分支同形，`word = 0x2b20` = `" + "`
    ///  * `0x3c6125 call *[0x9ba718] -> 0x91ded0` ⇒ `core::fmt`（`` 的格式说明在 `.rodata 0x19f3c8`）
    pub fn calculate_function_offset_str(text: &str, x0: f64, y0: f64, flip: bool) -> String {
        // `c = y0 - f(±x0)`（与 `correction()` 同一条算式，故直接复用）
        let c = correction(&crate::math_function::parse(text), x0, y0, flip);
        // 分支极性照抄 `ucomisd 0.0, c` + `jbe`：`c` 不小于 0（含 NaN、含 -0.0）都走 `+`
        if c < 0.0 {
            format!(" - {:.3}", -c)
        } else {
            format!(" + {:.3}", c)
        }
    }

    /// `GraphwarShotCalculator::calculate_corrected_function`（Linux **`0x3c5c50`** / arm64 `0x3b79a8`）
    ///
    /// = 原文**逐字** + 把原文里**未闭合的 `(`** 用 `)` 补齐 + 修正常数文本。
    ///
    /// 指令锚（x86-64 Linux）：
    ///  * `0x3c5c5e mov r15,rcx` / `0x3c5c61 mov r14,rdx` / `0x3c5c64 mov r12,rsi`
    ///    ⇒ 入参 = (sret, 文本指针, 文本长度, `&Point`, flip)
    ///  * `0x3c5c80 inc ebp`（遇 `(` 加深）/ `0x3c5d20 cmp edx,0x29` + `0x3c5d2b test ebp,ebp`
    ///    + `0x3c5d2d setg dl` + `0x3c5d30 sub ebp,edx` ⇒ 遇 `)` **只在计数 >0 时减**，
    ///    所以净未闭合数**永不为负**（多余的 `)` 原样留着，前面不会补 `(`）
    ///  * `0x3c5e1c`..`0x3c5e58` 把原文字节复制进新 `String`
    ///  * `0x3c5e90` 循环 `0x3c5e97 mov byte ptr [r13+rsi],0x29` ⇒ 每个未闭合的 `(` 补一个 `')'`
    ///  * `0x3c5d9a call *[0x9bacf0] -> 0x3c5f70` ⇒ 修正常数文本仍按**原文**算
    ///    （`rsi/r12`、`rdx/r14` 全程没被改写）
    ///  * `0x3c5dd8` 把 offset 串 `memcpy` 接到补齐后的文本后面
    ///
    /// ⚠️ 2026-11-28 之前我方（[`corrected_text`]）只做 `format!("{}{}", text, offset)`
    /// —— **不会补括号**。真二进制会（本轮值级对拍 `fixtures/denom_drawers_math_truth.tsv`
    /// 的 `V3-01..V3-07`：`"(x" -> "(x) + 1.000"`、`"(()" -> "(()) + 2.000"`、`"x)" -> "x) …"`）。
    /// 这是本轮修掉的一处**真实差异**（不是实现风格）。
    pub fn calculate_corrected_function(text: &str, x0: f64, y0: f64, flip: bool) -> String {
        // 净未闭合 `(` 数。照 `setg` 的极性：`)` 只在计数 > 0 时才抵一个。
        let mut depth = 0usize;
        for ch in text.chars() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    if depth > 0 {
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }
        // 容量 = 原文 + 待补的 `)` + 常数文本（原程序也是先按原文长度分配再按需扩容）
        let mut out = String::with_capacity(text.len() + depth + 8);
        out.push_str(text);
        for _ in 0..depth {
            out.push(')');
        }
        out.push_str(&Self::calculate_function_offset_str(text, x0, y0, flip));
        out
    }
}

#[cfg(test)]
mod correction_tests {
    use super::*;

    /// 真实抓包样本：士兵 234 用 `0.00` 开火，修正文本是 `0.00 + 4.638`。
    #[test]
    fn correction_matches_captured_text_234() {
        let x0 = 1.651220991201045_f64;
        let y0 = 4.638020825690482_f64;
        let txt = corrected_text("0.00", x0, y0, false);
        assert_eq!(txt, "0.00 + 4.638", "抓包里的修正文本");
    }

    /// 真实抓包样本：士兵 231 用空函数开火，修正文本是 ` + 10.990`。
    #[test]
    fn correction_matches_captured_text_231() {
        let x0 = -19.512760846799154_f64;
        let y0 = 10.99018276610812_f64;
        let txt = corrected_text("", x0, y0, false);
        assert_eq!(txt, " + 10.990", "抓包里的修正文本");
    }

    /// **符号**的回归：`c < 0` 时运算符是 `" - "`，不是 `" + -"`。
    ///
    /// 三条都是原始字节流抓包的**原文**（`docs/127` §2；`start_x/start_y` 取自同一回合的
    /// `FunctionPoints`）。旧实现（写死 `" + "`）在 182/203 这两条上会给出
    /// `" + -3.686"` / `" + -11.416"`，真机从没这么写过。
    #[test]
    fn correction_operator_carries_the_sign_of_the_constant() {
        // 帧 182/183：士兵 231（Team1 ⇒ flip=false）空函数 ⇒ c = y0 = -3.6861
        assert_eq!(
            corrected_text("", -13.429861913415694, -3.686102115928783, false),
            " - 3.686",
            "抓包帧 182"
        );
        // 帧 203/204：士兵 233（Team2 ⇒ flip=true）曲线 + 负常数
        assert_eq!(
            corrected_text("abs((abs((-0.57+x))+-4.85))", 14.305645069934501, -1.3905063945238432, true),
            "abs((abs((-0.57+x))+-4.85)) - 11.416",
            "抓包帧 203"
        );
        // 帧 143/144：士兵 230 空函数，同一条链上的另一个负样本
        assert_eq!(
            corrected_text("", -8.2808732224322, -3.4200779499591807, false),
            " - 3.420",
            "抓包帧 143"
        );
        // 正号那一侧照旧（帧 161/162，flip=true）
        assert_eq!(
            corrected_text("cos((0.19*(6.70+x)))", 11.962551145879328, 3.7443842094294837, true),
            "cos((0.19*(6.70+x))) + 3.204",
            "抓包帧 161"
        );
    }

    /// NaN 仍走 `+` 那一侧（`correction_text` 的既有行为，别被上面的符号分支带跑）。
    #[test]
    fn correction_keeps_the_nan_branch() {
        assert_eq!(corrected_text("0.00", 0.0, f64::NAN, false), "0.00 + NaN");
        assert_eq!(corrected_text("", 0.0, f64::NAN, false), " + NaN");
        // 正/负两侧的定值（c = 2.000 / -2.000）
        assert_eq!(corrected_text("0.00", 1.0, 2.0, false), "0.00 + 2.000");
        assert_eq!(corrected_text("0.00", 1.0, -2.0, false), "0.00 - 2.000");
    }

    /// 抓到的弹道终点必须落在我们算出的轨迹上（这两发都是水平直线）。
    #[test]
    fn captured_shot_endpoints_lie_on_our_trajectory() {
        // (起点 x, 起点 y, 抓到的终点 x, 终点 y, 是否反向)
        // 方向由实测确定：第一发向左（flip=true），第二发向右（flip=false）
        let cases = [
            (1.651220991201045_f64, 4.638020825690482_f64, -16.405029008797804_f64, 4.638_f64, true),
            (-19.512760846799154_f64, 10.99018276610812_f64, -12.040885846800286_f64, 10.99_f64, false),
        ];
        for (x0, y0, end_x, end_y, flip) in cases {
            let pts = shot_points("0.00", x0, y0, crate::SERVER_STEP, flip, Mode::Normal, true);
            assert!(!pts.is_empty());
            // 水平直线：整条轨迹 y 恒等于修正后的常数
            let c = y0; // f(x)=0 ⇒ 修正后恒为 y0
            assert!((pts[0].1 - c).abs() < 1e-9, "起点 y 不符");
            assert!(pts.iter().all(|p| (p.1 - c).abs() < 1e-9), "应恒为水平线");
            // 抓到的爆炸点 y 与轨迹一致（抓到的是三位小数）
            assert!((pts[0].1 - end_y).abs() < 5e-4, "终点 y 与轨迹不符: {} vs {}", pts[0].1, end_y);
            // 终点 x 必须落在轨迹覆盖的 x 区间内
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in &pts {
                lo = lo.min(p.0);
                hi = hi.max(p.0);
            }
            assert!(end_x >= lo - 1e-6 && end_x <= hi + 1e-6,
                    "终点 x={end_x} 不在轨迹 x 区间 [{lo}, {hi}] 内");
        }
    }
}

#[cfg(test)]
mod denom_truth_tests {
    //! T108：`fixtures/denom_drawers_math_truth.tsv` 的**值级对拍**。
    //!
    //! 真值来源：Linux x86-64 `libgraphwarrust.so` 的**真机器码**在 unicorn 下直跑
    //! （`tools/crossarch_emu.py::X86` → `.reference/analysis-repo/analysis/gwemu.py`，
    //! 真 glibc libm）—— **不是**本实现的输出，也不是 `cargo test` 自证。
    //! 生成/复算：`.reference/venv/bin/python tools/denom_drawers_truth.py`（`--check` 只比对）。
    //!
    //! 覆盖（预注册 `fixtures/denom_drawers_math_plan.txt` §3）：
    //!  * `S1` = `calculate_function`（`0x3c2530`）20 组：点数 + 整条点数组 FNV + 首/末点逐位；
    //!  * `S2S3` = `calculate_corrected_function`（`0x3c5c50`）+ `calculate_function_offset_str`
    //!    （`0x3c5f70`）33 组：两条返回的 `String` **逐字节**（长度 + 内容）相同；
    //!  * `S2S3-extra` = 本轮读反汇编后**新发现**的行为（未闭合 `(` 补 `)`；`-0.0`/`±inf` 边界）
    //!    11 组，标注为"不在预注册输入集内"。
    use super::*;

    const TRUTH: &str = include_str!("../fixtures/denom_drawers_math_truth.tsv");

    // ---------------------------------------------------------------- TSV 读
    fn table() -> (Vec<&'static str>, Vec<Vec<&'static str>>) {
        let mut cols: Vec<&'static str> = Vec::new();
        let mut rows: Vec<Vec<&'static str>> = Vec::new();
        for line in TRUTH.lines() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&'static str> = line.split('\t').collect();
            if cols.is_empty() {
                cols = f;
            } else {
                rows.push(f);
            }
        }
        assert!(!cols.is_empty(), "真值表没有表头 —— 夹具坏了");
        (cols, rows)
    }

    fn col(cols: &[&'static str], name: &str) -> usize {
        cols.iter()
            .position(|c| *c == name)
            .unwrap_or_else(|| panic!("真值表缺列 {name}"))
    }

    fn get<'a>(row: &[&'a str], i: usize) -> &'a str {
        row.get(i).copied().unwrap_or("")
    }

    // ---------------------------------------------------------------- 工具
    fn bits(h: &str) -> f64 {
        f64::from_bits(u64::from_str_radix(h.trim(), 16).expect("f64 位模式"))
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for b in bytes {
            h = (h ^ (*b as u64)).wrapping_mul(0x100_0000_01B3);
        }
        h
    }

    fn points_bytes(pts: &[(f64, f64)]) -> Vec<u8> {
        let mut v = Vec::with_capacity(pts.len() * 16);
        for (x, y) in pts {
            v.extend_from_slice(&x.to_le_bytes());
            v.extend_from_slice(&y.to_le_bytes());
        }
        v
    }

    fn hex_encode(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        let s = s.trim();
        assert!(s.len() % 2 == 0, "十六进制长度不是偶数: {s:?}");
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// token tag -> `Function`（原程序 tag 编号，见 `src/lib.rs::Function` 的 `repr(u64)`）。
    fn tag_fn(t: u64) -> Function {
        match t {
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
            other => panic!("未知 token tag {other}"),
        }
    }

    /// `tag:valuebits;...` -> token 列表（空串 = 空 token 流）。
    fn parse_tokens(field: &str) -> Vec<Token> {
        if field.is_empty() {
            return Vec::new();
        }
        field
            .split(';')
            .map(|t| {
                let (tag, val) = t.split_once(':').expect("tag:value");
                Token { tag: tag_fn(u64::from_str_radix(tag, 10).expect("tag")), value: bits(val) }
            })
            .collect()
    }

    // ---------------------------------------------------------------- ① 采样器
    /// `calculate_function`（`0x3c2530`）20 组值级对拍。
    ///
    /// 关键点：空 token 流在**真采样器**里不是"没有弹道"，而是一条水平线
    /// （`V1-06`/`V1-07`/`V1-08` 与常量 0 的 `V1-01` **同一条 sha**）。
    /// ⇒ 我方 `shot_points()` 里"空文本在 Normal 局早退"是**组合方式**的推断
    /// （`docs/06` §156.5），不是采样器语义；这里测的是采样器本身。
    #[test]
    fn calculate_function_matches_real_machine_code() {
        let (cols, rows) = table();
        let c_sec = col(&cols, "section");
        let c_case = col(&cols, "case");
        let t_tok = col(&cols, "tokens");
        let t_x = col(&cols, "x");
        let t_y = col(&cols, "y");
        let t_step = col(&cols, "step");
        let t_flip = col(&cols, "flip");
        let t_mode = col(&cols, "mode");
        let t_n = col(&cols, "n");
        let t_fnv = col(&cols, "fnv");
        let t_first = col(&cols, "first");
        let t_last = col(&cols, "last");

        let mut checked = 0;
        for row in &rows {
            if get(row, c_sec) != "S1" {
                continue;
            }
            let case = get(row, c_case);
            let toks = parse_tokens(get(row, t_tok));
            let mode = if get(row, t_mode) == "1" { Mode::Ode } else { Mode::Normal };
            let flip = get(row, t_flip) != "0";
            let pts = GraphwarShotCalculator::calculate_function(
                &toks,
                flip,
                mode,
                bits(get(row, t_x)),
                bits(get(row, t_y)),
                bits(get(row, t_step)),
            );
            let want_n: usize = get(row, t_n).parse().expect("n");
            assert_eq!(pts.len(), want_n, "{case}: 点数与真采样器 0x3c2530 不一致");
            let want_fnv = u64::from_str_radix(get(row, t_fnv), 16).expect("fnv");
            assert_eq!(
                fnv1a64(&points_bytes(&pts)),
                want_fnv,
                "{case}: 整条点数组与真机器码不一致（逐位判据）"
            );
            if want_n > 0 {
                let pt = |field: &str| -> (f64, f64) {
                    let (a, b) = field.split_once(',').expect("x,y");
                    (bits(a), bits(b))
                };
                let (fx, fy) = pt(get(row, t_first));
                assert_eq!(
                    (pts[0].0.to_bits(), pts[0].1.to_bits()),
                    (fx.to_bits(), fy.to_bits()),
                    "{case}: 首点与真机器码不一致"
                );
                let (lx, ly) = pt(get(row, t_last));
                assert_eq!(
                    (pts[pts.len() - 1].0.to_bits(), pts[pts.len() - 1].1.to_bits()),
                    (lx.to_bits(), ly.to_bits()),
                    "{case}: 末点与真机器码不一致"
                );
            }
            checked += 1;
        }
        assert_eq!(checked, 20, "S1 真值行数变了（预注册 §3.1 是 20 组）");
    }

    // ---------------------------------------------------------------- ② 修正量外壳
    /// `calculate_corrected_function`（`0x3c5c50`）+ `calculate_function_offset_str`
    /// （`0x3c5f70`）44 组的**逐字节**对拍（`S2S3` 33 + `S2S3-extra` 11）。
    #[test]
    fn corrected_function_matches_real_machine_code() {
        let (cols, rows) = table();
        let c_sec = col(&cols, "section");
        let c_case = col(&cols, "case");
        let t_text = col(&cols, "texts");
        let t_x = col(&cols, "x");
        let t_y = col(&cols, "y");
        let t_flag = col(&cols, "flag");
        let t_clen = col(&cols, "corr_len");
        let t_chex = col(&cols, "corr_hex");
        let t_olen = col(&cols, "off_len");
        let t_ohex = col(&cols, "off_hex");

        let (mut n_main, mut n_extra) = (0, 0);
        for row in &rows {
            let sec = get(row, c_sec);
            if sec != "S2S3" && sec != "S2S3-extra" {
                continue;
            }
            let case = get(row, c_case);
            let text = String::from_utf8(hex_decode(get(row, t_text))).expect("UTF-8");
            let x0 = bits(get(row, t_x));
            let y0 = bits(get(row, t_y));
            // 第 4 参在真二进制里按 **bool** 解释（`r8b`；`flag=0x100` 的低字节是 0 ⇒ 不 flip，
            // 见 V2-14/V2-15）—— 不是 mode。
            let flip = (get(row, t_flag).parse::<u32>().expect("flag") & 0xff) != 0;

            let corr = GraphwarShotCalculator::calculate_corrected_function(&text, x0, y0, flip);
            let off = GraphwarShotCalculator::calculate_function_offset_str(&text, x0, y0, flip);

            assert_eq!(
                corr.as_bytes().len(),
                get(row, t_clen).parse::<usize>().expect("corr_len"),
                "{case}: corrected 文本长度与真二进制不一致"
            );
            assert_eq!(
                hex_encode(corr.as_bytes()),
                get(row, t_chex),
                "{case}: corrected 文本逐字节与真二进制不一致（{:?}）",
                corr
            );
            assert_eq!(
                off.as_bytes().len(),
                get(row, t_olen).parse::<usize>().expect("off_len"),
                "{case}: offset 文本长度与真二进制不一致"
            );
            assert_eq!(
                hex_encode(off.as_bytes()),
                get(row, t_ohex),
                "{case}: offset 文本逐字节与真二进制不一致（{:?}）",
                off
            );
            if sec == "S2S3" {
                n_main += 1;
            } else {
                n_extra += 1;
            }
        }
        assert_eq!(n_main, 33, "S2S3 真值行数变了（预注册 §3.2 是 33 组）");
        assert_eq!(n_extra, 11, "S2S3-extra 行数变了（本轮追加 11 组）");
    }

    /// 可读版：真二进制给的三条关键行为（值取自 `fixtures/denom_drawers_math_truth.tsv`）。
    #[test]
    fn corrected_function_quirks_from_real_binary() {
        // ① 原文**逐字**保留（大写/空格都不规范化）：`X^3`, `x ^ 3`, `x^3   `（V2-08/07/09）
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("X^3", 1.0, 0.0, false),
            "X^3 - 1.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("x ^ 3", 1.0, 0.0, false),
            "x ^ 3 - 1.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("x^3   ", 1.0, 0.0, false),
            "x^3    - 1.000"
        );
        // ② 未闭合的 `(` 会被补 `)`，多余的 `)` 原样留着（V3-01/02/03/05/06/07）
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("(x", 1.0, 2.0, false),
            "(x) + 1.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("((x", 1.0, 2.0, false),
            "((x)) + 1.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("x)", 1.0, 2.0, false),
            "x) + 1.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("(()", 1.0, 2.0, false),
            "(()) + 2.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("x(", 1.0, 2.0, false),
            "x() + 2.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_corrected_function("))x((", 1.0, 2.0, false),
            "))x(()) + 2.000"
        );
        // ③ 边界：`c = -0.0` 走 `+` 分支但把 `-0.0` 原样格式化（V3-08）；
        //    `±inf` 打印 `inf`（V3-09/10）；NaN 走 `+`（V2-21）
        assert_eq!(
            GraphwarShotCalculator::calculate_function_offset_str("0", 1.0, -0.0, false),
            " + -0.000"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_function_offset_str("x", 1e308, -1e308, false),
            " - inf"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_function_offset_str("x", -1e308, 1e308, false),
            " + inf"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_function_offset_str("sqrt(-1)", 1.0, 0.0, false),
            " + NaN"
        );
        // ④ 第 4 参是 **bool**，不是 mode：低字节 0 的 0x100 ⇒ 不反向（V2-14 真值是 `+ 2.402`）
        assert_eq!(
            GraphwarShotCalculator::calculate_function_offset_str("sin(x)", 2.5, 3.0, false),
            " + 2.402"
        );
        assert_eq!(
            GraphwarShotCalculator::calculate_function_offset_str("sin(x)", 2.5, 3.0, true),
            " + 3.598"
        );
        // ⑤ 旧 API 也跟着修好了（之前只 `format!("{}{}")`，不补括号）
        assert_eq!(corrected_text("(x", 1.0, 2.0, false), "(x) + 1.000");
    }
}
