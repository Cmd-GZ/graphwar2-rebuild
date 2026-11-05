//! 随机函数生成器：AI 的初始种群与变异都靠它抽 token。
//!
//! 全部数值直接读自二进制（`docs/71-token-weights.md`）：
//!
//! * 加权抽样用 `random % 85`，按区间映射到 tag；
//! * 常数 token = `N(0,1) × 10.2`（缩放常数在 `.rodata 0x19f758`）；
//! * 权重表见 [`TABLE`]，合计 [`TOTAL_WEIGHT`] = 85。
//!
//! 运行时用 40 万样本核过一遍（`tools/hook_token_tags.js`），
//! 实测频率与下表逐项吻合到小数点后第二位。

use crate::levelgen::normal;
use crate::{Function, Token};

/// 权重总和，也就是二进制里 `random % 0x55` 的模数。
pub const TOTAL_WEIGHT: u32 = 85;

/// 常数 token 的标准差：`N(0,1) × 10.2`。
///
/// GW1 的 `RANDOM_FUNC_VALUE_MEAN` 同样是 10.2，两代一致。
pub const CONST_SCALE: f64 = 10.2;

/// `(tag, 权重)`，按分派表里的下标顺序排列。
///
/// 注意 `VarX` 的权重是 10 而不是 6：分派表里 11–16 走立即数分支、
/// 17–20 走"栈上预存值"分支，普通模式下两者都产出 `x`。
/// 运行时实测 `x` 占 11.77%（= 10/85），据此确认。
pub const TABLE: [(Function, u32); 15] = [
    (Function::Const, 11),
    (Function::VarX, 10),
    (Function::Add, 10),
    (Function::Sub, 5),
    (Function::Mul, 11),
    (Function::Divide, 7),
    (Function::Pow, 2),
    (Function::Sqrt, 1),
    (Function::Log, 1),
    (Function::Ln, 1),
    (Function::Exp, 6),
    (Function::Abs, 7),
    (Function::Sin, 7),
    (Function::Cos, 5),
    (Function::Tan, 1),
];

/// 抽一个函数长度（token 数）。
///
/// 二进制里是 `trunc(N(0,1) * 5.0 + 10.0)`（`0x4ef08e` 的 `mulsd 5.0` +
/// `0x4ef096` 的 `addsd 10.0` + `0x4ef0b2` 的 `cvttsd2si`；
/// 中间那两个 `maxsd/minsd` 只是 i32 上下界，防截断溢出）。
///
/// 正态取负或很小时会得到 ≤0 的长度，调用方应把它当作"最短的合法表达式"处理
/// （实测最小值为 2，见 `docs/76`）。
pub fn random_length(next: &mut dyn FnMut() -> u64) -> i32 {
    (normal(next) * 5.0 + 10.0) as i32
}

/// 抽一个 tag（不含常数的取值）。
///
/// 对应二进制 `FUN_005ef020` 里的 `idx = random % 85` 加区间比较；
/// `next` 是 ChaCha 的取数闭包。
pub fn draw_tag(next: &mut dyn FnMut() -> u64) -> Function {
    let mut idx = (next() % u64::from(TOTAL_WEIGHT)) as u32;
    for &(tag, weight) in TABLE.iter() {
        if idx < weight {
            return tag;
        }
        idx -= weight;
    }
    // 权重和若不是 85 才会走到这里；保底返回最后一个，便于测试发现表被改坏。
    debug_assert!(false, "TABLE 权重之和应等于 TOTAL_WEIGHT");
    Function::Tan
}

/// 抽一个完整 token；抽到常数时按 `N(0,1) × 10.2` 取值。
pub fn draw_token(next: &mut dyn FnMut() -> u64) -> Token {
    let tag = draw_tag(next);
    if tag == Function::Const {
        Token::constant(normal(next) * CONST_SCALE)
    } else {
        Token::op(tag)
    }
}

/// 变异算子（`FUN_005ee720`，见 `docs/73`）。
///
/// 在 token 序列上挑一个"值 token"（`x` / `y` / 常数），然后：
///
/// * 挑到常数   → 一半概率原地乘 `(1 + N(0,1))`（这就是常数漂移的来源），
///                一半概率换成新的 `N(0,1)×10.2`；
/// * 挑到 `y` / `x` → 把它**包一层**：`v` 变成 `(op const v)`，
///   `op` 按 50/25/25 取 `+` / `*` / `/`，`const` 仍是 `N(0,1)×10.2`。
///
/// `y` 分支原本我以为是"只插一个常数"，那样会得到 `const y`（两个值相邻，
/// 在前缀式里非法）。重读伪代码后发现它**后面接着同一段包一层逻辑**，
/// 所以 `y` 与 `x` 走同一条路，见 `docs/73` §5.1。
///
/// 返回是否真的改动了序列，便于调用方统计。
pub fn mutate(tokens: &mut Vec<Token>, next: &mut dyn FnMut() -> u64) -> bool {
    let positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t.tag, Function::VarX | Function::VarY | Function::Const))
        .map(|(i, _)| i)
        .collect();
    if positions.is_empty() {
        return false;
    }
    let pos = positions[(next() % positions.len() as u64) as usize];
    match tokens[pos].tag {
        Function::Const => {
            // 二进制里是"抽一个随机 int，看符号"决定走哪条路
            if (next() as i32) >= 0 {
                tokens[pos].value *= 1.0 + normal(next);
            } else {
                tokens[pos].value = normal(next) * CONST_SCALE;
            }
            true
        }
        // `y`、`x` 以及其它：统一"包一层"
        _ => {
            let konst = Token::constant(normal(next) * CONST_SCALE);
            tokens.insert(pos, konst);
            // 抽随机 int 定算子：>=0 → `+`；<0 再抽一次，<0 → `*`，否则 `/`
            let first = next() as i32;
            let op = if first >= 0 {
                Function::Add
            } else if (next() as i32) < 0 {
                Function::Mul
            } else {
                Function::Divide
            };
            tokens.insert(pos, Token::op(op));
            true
        }
    }
}

// ---------------------------------------------------------------------------
// 结构还原版 AI（docs/67、docs/75、docs/76）
// ---------------------------------------------------------------------------

/// 六档难度的 GA 参数（**实测**：从 `FUN_0061b440` 的实参直读，见 `docs/67`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    /// 每回合的代数（外层循环次数）。
    pub generations: u32,
    /// 种群大小 = 实参 `a5` × 5。
    pub population: u32,
    /// 精英个数（栈参数，六档恒为 5）。
    pub elite: u32,
    /// 复杂度硬上限（`V > limit` 罚 5e7），六档 5/5/10/15/25/200。
    pub limit: u32,
}

/// 难度 1..=6 → 实测参数。越界按 6 处理。
pub fn params(level: u8) -> Params {
    let (generations, population, limit) = match level {
        1 => (3, 50, 5),
        2 => (10, 100, 5),
        3 => (25, 150, 10),
        4 => (100, 200, 15),
        5 => (250, 250, 25),
        _ => (1000, 250, 200),
    };
    Params { generations, population, elite: 5, limit }
}

/// 一个个体：token 序列 + 适应度。
#[derive(Debug, Clone, PartialEq)]
pub struct Individual {
    pub tokens: Vec<Token>,
    pub score: f64,
}

/// token 的"需要的操作数"：值 0、二元 2、一元 1。
fn arity(tag: Function) -> i64 {
    match tag {
        Function::Const | Function::VarX | Function::VarY | Function::VarDy => 0,
        Function::Add | Function::Sub | Function::Mul | Function::Divide | Function::Pow => 2,
        _ => 1,
    }
}

/// 是不是一棵完整、合法的前缀表达式。
pub fn is_well_formed(tokens: &[Token]) -> bool {
    let mut need: i64 = 1;
    for t in tokens {
        need -= 1;
        need += arity(t.tag);
        if need < 0 {
            return false;
        }
    }
    need == 0
}

/// 把任意 token 序列修成合法前缀式。
///
/// ⚠️ **这一步是假设**：GW2 里"raw token 流 → 合法表达式"的修复代码还没定位到
/// （`docs/76` §4.5）。这里按 GW1 `adjustFunction` 的语义实现：
/// 走到表达式完整就**截断**，走完仍缺操作数就**补常数**。
pub fn repair(tokens: &mut Vec<Token>, next: &mut dyn FnMut() -> u64) {
    let mut need: i64 = 1;
    let mut cut = tokens.len();
    for (i, t) in tokens.iter().enumerate() {
        need -= 1;
        need += arity(t.tag);
        if need == 0 {
            cut = i + 1;
            break;
        }
    }
    tokens.truncate(cut);
    // 走完还没闭合：补 need 个常数叶子
    need = 1;
    for t in tokens.iter() {
        need -= 1;
        need += arity(t.tag);
    }
    while need > 0 {
        tokens.push(Token::constant(normal(next) * CONST_SCALE));
        need -= 1;
    }
}

/// 抽一个随机函数：先按 `trunc(N(0,1)×5+10)` 定长度，再抽那么多个 token，最后修复。
///
/// 长度公式与"抽多少发多少"都是**实测**（`docs/76`）；修复是**假设**（见 [`repair`]）。
pub fn random_function(next: &mut dyn FnMut() -> u64) -> Vec<Token> {
    let n = random_length(next).max(1) as usize;
    let mut tokens: Vec<Token> = (0..n).map(|_| draw_token(next)).collect();
    repair(&mut tokens, next);
    tokens
}

/// 交叉：把两个父代的 token 序列各取一段拼起来，再修复。
///
/// 二进制里真正的交叉是 `FUN_003be7d0`（按 token 向量切片），
/// 这里的实现只保留"各取一段"这个**可观测特征**，具体切点规则**未还原**。
pub fn crossover(
    a: &[Token],
    b: &[Token],
    next: &mut dyn FnMut() -> u64,
) -> Vec<Token> {
    let mut out = Vec::new();
    if !a.is_empty() {
        let ca = (next() % a.len() as u64) as usize + 1;
        out.extend_from_slice(&a[..ca]);
    }
    if !b.is_empty() {
        let cb = (next() % b.len() as u64) as usize;
        out.extend_from_slice(&b[cb..]);
    }
    repair(&mut out, next);
    out
}

/// 走一代（`docs/75` §2.5 的实测结构）。
///
/// 实测：每个外层迭代 = **1 个克隆（数组头部按索引顺序）+ 4 个工厂子代**，
/// 外层 0.2×种群 次 ⇒ `0.2n + 4×0.2n = n`。
///
/// ⚠️ 两条仍是假设：① 头部"已按质量排序"只有间接证据；
/// ② 工厂内部的交叉切点规则未还原（见 [`crossover`]）。
/// `elite`（栈参数 5）的用途仍未定，这里**不使用**它。
pub fn next_generation<F>(
    pop: &[Individual],
    p: &Params,
    next: &mut dyn FnMut() -> u64,
    mut score: F,
) -> Vec<Individual>
where
    F: FnMut(&[Token]) -> f64,
{
    let mut sorted: Vec<Individual> = pop.to_vec();
    sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    if sorted.is_empty() {
        return Vec::new();
    }

    let n = p.population as usize;
    let clones = n / 5; // 0.2 × 种群
    let mut out: Vec<Individual> = Vec::with_capacity(n);

    for i in 0..clones {
        // ① 克隆头部第 i 个（索引顺序，实测）
        out.push(sorted[i.min(sorted.len() - 1)].clone());
        // ② 4 个工厂子代（实测：内层循环 = 4 × 外层）
        for _ in 0..4 {
            let a = (next() % sorted.len() as u64) as usize;
            let b = (next() % sorted.len() as u64) as usize;
            let tokens = if sorted[b].tokens.len() < 20 {
                // 第二个父代太短 → 重生（实测 cmp $0x14）
                random_function(next)
            } else {
                let mut child = crossover(&sorted[a].tokens, &sorted[b].tokens, next);
                mutate(&mut child, next);
                child
            };
            out.push(Individual { tokens, score: 0.0 });
        }
    }
    out.truncate(n);
    for ind in out.iter_mut() {
        ind.score = score(&ind.tokens);
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// 跑满 `params.generations` 代，返回按分数降序的最终种群。
///
/// 这就是"一回合思考"的骨架：调用方负责提供初始种群与适应度闭包。
pub fn evolve<F>(
    mut pop: Vec<Individual>,
    p: &Params,
    next: &mut dyn FnMut() -> u64,
    mut score: F,
) -> Vec<Individual>
where
    F: FnMut(&[Token]) -> f64,
{
    for ind in pop.iter_mut() {
        if ind.score == 0.0 {
            ind.score = score(&ind.tokens);
        }
    }
    for _ in 0..p.generations {
        pop = next_generation(&pop, p, next, &mut score);
    }
    pop
}

/// 造一个初始种群。
pub fn initial_population(p: &Params, next: &mut dyn FnMut() -> u64) -> Vec<Individual> {
    (0..p.population)
        .map(|_| Individual { tokens: random_function(next), score: 0.0 })
        .collect()
}

// ---------------------------------------------------------------------------
// 适应度（docs/74 的公式，接上路径采样与命中判定）
// ---------------------------------------------------------------------------

/// 一次评分的上下文。
pub struct ScoringContext<'a> {
    pub x0: f64,
    pub y0: f64,
    pub flip: bool,
    /// 采样步长：粗评 `0.5`、精评 `0.01`（`docs/74` §1.1 的 `X`）。
    pub step: f64,
    /// 障碍掩膜（只含障碍）。
    pub mask: &'a [u8],
    /// 目标士兵（**不含射手本人**）。
    pub targets: &'a [crate::settle::Target],
    /// 复杂度硬上限（`V > limit` 罚 5e7）。
    pub limit: u32,
}

/// 适应度：`命中×2e6 − 1e6 − V²/100 − (V > limit ? 5e7 : 0)`。
///
/// **实测**（`docs/74`）：常规对局里没有"友军惩罚"项。
/// `V` 就是 token 数；`hits` 是这次弹道判死的士兵数。
pub fn fitness(tokens: &[Token], ctx: &ScoringContext) -> f64 {
    use crate::path::{calculate, Mode};
    use crate::plane::is_occupied;
    use crate::shot::{correction, shifted_tokens};

    let v = tokens.len();
    let mut score = -1_000_000.0 - (v * v) as f64 / 100.0;
    if v as u32 > ctx.limit {
        score -= 50_000_000.0;
    }
    if tokens.is_empty() {
        return score;
    }

    // 修正后沿弹道走一遍：先判致死（穿过即死、不挡弹道），再判障碍阻挡
    let c = correction(tokens, ctx.x0, ctx.y0, ctx.flip);
    let shifted = shifted_tokens(tokens, c);
    let points = calculate(&shifted, ctx.x0, ctx.y0, ctx.step, ctx.flip, Mode::Normal);

    let mut killed: Vec<i64> = Vec::new();
    for &(px, py) in &points {
        for t in ctx.targets {
            if killed.contains(&t.id) {
                continue;
            }
            let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
            if d <= t.radius {
                killed.push(t.id);
            }
        }
        if is_occupied(ctx.mask, px, py) {
            break;
        }
    }
    score + killed.len() as f64 * 2_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::ChaCha12;

    fn stream(seed: u64) -> impl FnMut() -> u64 {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        let mut r = ChaCha12::from_seed(&key);
        move || r.next_u64()
    }

    #[test]
    fn weights_sum_to_85() {
        let sum: u32 = TABLE.iter().map(|(_, w)| *w).sum();
        assert_eq!(sum, TOTAL_WEIGHT, "权重之和必须等于模数 85");
        assert_eq!(TABLE.len(), 15, "分派表共 15 个非零区间");
    }

    #[test]
    fn distribution_matches_table() {
        let mut next = stream(0x9E37_79B9_7F4A_7C15);
        const N: u32 = 170_000; // 2000 轮 × 85
        let mut got = [0u32; 20];
        for _ in 0..N {
            got[draw_tag(&mut next) as usize] += 1;
        }
        for &(tag, weight) in TABLE.iter() {
            let expect = (N * weight) / TOTAL_WEIGHT;
            let actual = got[tag as usize];
            let diff = (actual as i64 - expect as i64).unsigned_abs();
            // 34 次抽样里单点标准差约 sqrt(N·p·(1-p))，这里放宽到 5%
            assert!(
                diff < expect as u64 / 20 + 50,
                "{tag:?}: 期望 {expect}，实测 {actual}"
            );
        }
    }

    #[test]
    fn constants_are_normal_scaled_by_10_2() {
        let mut next = stream(0x1234_5678_9ABC_DEF1);
        let mut values = Vec::new();
        while values.len() < 20_000 {
            let t = draw_token(&mut next);
            if t.tag == Function::Const {
                values.push(t.value);
            }
        }
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let sd = var.sqrt();
        assert!(mean.abs() < 1.0, "常数均值应接近 0，实测 {mean}");
        assert!(
            (sd - CONST_SCALE).abs() < 0.8,
            "常数标准差应接近 {CONST_SCALE}，实测 {sd}"
        );
    }

    #[test]
    fn never_emits_y_or_neg() {
        // 分派表里没有 VarY / VarDy / UnaryMinus / 括号，NORMAL 模式不应出现
        let mut next = stream(42);
        for _ in 0..10_000 {
            let tag = draw_tag(&mut next);
            assert!(
                tag != Function::VarY
                    && tag != Function::VarDy
                    && tag != Function::UnaryMinus
                    && tag != Function::LeftBracket
                    && tag != Function::RightBracket,
                "不应抽到 {tag:?}"
            );
        }
    }

    #[test]
    fn mutate_does_nothing_without_value_tokens() {
        let mut tokens = vec![Token::op(Function::Add), Token::op(Function::Sin)];
        let mut next = stream(7);
        assert!(!mutate(&mut tokens, &mut next), "没有值 token 时不应改动");
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn mutate_wraps_y_like_x() {
        let mut tokens = vec![Token::op(Function::VarY)];
        let mut next = stream(11);
        assert!(mutate(&mut tokens, &mut next));
        assert_eq!(tokens.len(), 3, "y 应与 x 一样被包成 (op const y)");
        assert!(matches!(
            tokens[0].tag,
            Function::Add | Function::Mul | Function::Divide
        ));
        assert_eq!(tokens[1].tag, Function::Const);
        assert_eq!(tokens[2].tag, Function::VarY);
    }

    #[test]
    fn mutate_constant_is_multiplicative_drift_or_refresh() {
        // 反复变异同一个常数：结果必须是"乘上 (1+N)"或"换成新的 N(0,10.2)"之一，
        // 所以绝不能保持原值不变。
        let mut changed = 0;
        for seed in 0..200u64 {
            let mut tokens = vec![Token::constant(3.0)];
            let mut next = stream(seed * 7919 + 13);
            assert!(mutate(&mut tokens, &mut next));
            if tokens[0].value != 3.0 {
                changed += 1;
            }
        }
        assert_eq!(changed, 200, "每次变异都应改变常数");
    }

    #[test]
    fn mutate_spreads_constants_over_many_rounds() {
        // 乘性随机游走：多轮之后常数会明显偏离初值（这正是 1770417.48 的成因）
        let mut next = stream(0xDEAD_BEEF);
        let mut tokens = vec![Token::constant(10.0)];
        for _ in 0..400 {
            let mut t = tokens.clone();
            mutate(&mut t, &mut next);
            if t[0].tag == Function::Const && t.len() == 1 {
                tokens = t;
            }
        }
        assert!(
            (tokens[0].value - 10.0).abs() > 1e-9,
            "多轮变异后常数不应仍是原值"
        );
    }

    #[test]
    fn mutate_wraps_x_in_an_operation() {
        let mut next = stream(0x5EED);
        let mut tokens = vec![Token::op(Function::VarX)];
        assert!(mutate(&mut tokens, &mut next));
        assert_eq!(tokens.len(), 3, "x 应被包成 (op const x)");
        assert!(
            matches!(tokens[0].tag, Function::Add | Function::Mul | Function::Divide),
            "最外层应是 +/*/ 之一，实测 {:?}",
            tokens[0].tag
        );
        assert_eq!(tokens[1].tag, Function::Const, "中间是常数");
        assert_eq!(tokens[2].tag, Function::VarX, "原来是 x 的位置仍在最后");
    }

    #[test]
    fn mutation_only_grows_or_keeps_the_sequence() {
        // 注意：y 分支插入常数后会短暂出现"两个值相邻"的形态，
        // 这在前缀式里是不合法的 —— 说明调用方后面应该还有一趟修复
        // （GW1 里对应 adjustFunction）。这里只断言"不会越变越短、不会 panic"。
        let mut next = stream(0xC0FFEE);
        let mut tokens = vec![Token::op(Function::VarX)];
        let mut prev_len = tokens.len();
        for round in 0..200 {
            mutate(&mut tokens, &mut next);
            assert!(
                tokens.len() >= prev_len,
                "第 {} 轮变异后长度变小了：{} -> {}",
                round + 1,
                prev_len,
                tokens.len()
            );
            prev_len = tokens.len();
        }
    }

    #[test]
    fn random_length_is_normal_10_5() {
        let mut next = stream(0x1CE_B00D);
        let n = 40_000;
        let mut sum = 0.0;
        let mut sq = 0.0;
        let mut count = 0;
        let mut min_seen = i32::MAX;
        for _ in 0..n {
            let len = random_length(&mut next);
            sum += len as f64;
            sq += (len as f64) * (len as f64);
            count += 1;
            min_seen = min_seen.min(len);
        }
        let mean = sum / count as f64;
        let sd = (sq / count as f64 - mean * mean).sqrt();
        // cvttsd2si 向零截断，连续分布截断后均值系统性下移约 0.5
        assert!((mean - 9.5).abs() < 0.3, "截断后均值应接近 9.5，实测 {mean}");
        assert!((sd - 5.0).abs() < 0.4, "标准差应接近 5，实测 {sd}");
        assert!(min_seen < 0, "正态取负时长度可以为负（实测最小 {min_seen}）");
    }

    #[test]
    fn mutations_keep_the_sequence_well_formed() {
        // y 与 x 统一走"包一层"之后，反复变异应始终保持合法前缀式。
        // 判定方式：从左到右走一遍，needed 从 1 开始，
        // 叶子 -1、算子 +(arity-1)，走完必须恰好为 0。
        fn well_formed(tokens: &[Token]) -> bool {
            let mut needed: i64 = 1;
            for t in tokens {
                match t.tag {
                    Function::VarX | Function::VarY | Function::Const => needed -= 1,
                    Function::Add
                    | Function::Sub
                    | Function::Mul
                    | Function::Divide
                    | Function::Pow => needed += 1,
                    _ => {} // 一元算子：arity 1 → +0
                }
                if needed < 0 {
                    return false;
                }
            }
            needed == 0
        }

        let mut next = stream(0xC0FFEE);
        let mut tokens = vec![Token::op(Function::VarX)];
        for round in 0..300 {
            mutate(&mut tokens, &mut next);
            assert!(
                well_formed(&tokens),
                "第 {} 轮变异后不是合法前缀式：{} 个 token",
                round + 1,
                tokens.len()
            );
        }
    }

    #[test]
    fn repair_fixes_arbitrary_sequences() {
        let mut next = stream(123);
        for seed in 0..200u64 {
            let mut d = vec![];
            let mut r = ChaCha12::from_seed(&{
                let mut k = [0u8; 32];
                k[..8].copy_from_slice(&(seed * 31 + 7).to_le_bytes());
                k
            });
            for _ in 0..(seed % 12 + 1) {
                d.push(draw_token(&mut || r.next_u64()));
            }
            repair(&mut d, &mut next);
            assert!(is_well_formed(&d), "修复后应合法：{d:?}");
        }
    }

    #[test]
    fn random_function_is_always_well_formed() {
        let mut next = stream(0xBEEF);
        for _ in 0..500 {
            let f = random_function(&mut next);
            assert!(!f.is_empty(), "不应生成空函数");
            assert!(is_well_formed(&f), "生成的函数应合法：{f:?}");
        }
    }

    #[test]
    fn params_match_measured_table() {
        assert_eq!(params(1), Params { generations: 3, population: 50, elite: 5, limit: 5 });
        assert_eq!(params(3), Params { generations: 25, population: 150, elite: 5, limit: 10 });
        assert_eq!(params(6), Params { generations: 1000, population: 250, elite: 5, limit: 200 });
    }

    #[test]
    fn next_generation_keeps_size_and_reuses_head() {
        let p = params(1); // 种群 50 → 克隆 10，工厂 40
        let mut next = stream(0xFEED);
        let mut pop = initial_population(&p, &mut next);
        for (i, ind) in pop.iter_mut().enumerate() {
            ind.score = i as f64;
        }
        let top: Vec<Vec<Token>> = {
            let mut s = pop.clone();
            s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            s.iter().take(10).map(|i| i.tokens.clone()).collect()
        };
        let g1 = next_generation(&pop, &p, &mut next, |t| t.len() as f64);
        assert_eq!(g1.len(), p.population as usize, "种群规模应不变");
        for t in &top {
            assert!(g1.iter().any(|i| &i.tokens == t), "头部个体应被克隆保留");
        }
    }

    #[test]
    fn evolve_runs_requested_generations() {
        let p = Params { generations: 3, population: 10, elite: 5, limit: 200 };
        let mut next = stream(99);
        let pop = initial_population(&p, &mut next);
        let out = evolve(pop, &p, &mut next, |t| t.len() as f64);
        assert_eq!(out.len(), 10);
        // 结果应按分数降序
        for w in out.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }

    #[test]
    fn evolve_can_learn_to_hit_a_target() {
        // 端到端：射手在 (10,0)，敌人在 (0,0)，空场。
        // 初始种群是随机函数，几乎不可能命中（分数约 -1e6）；
        // 跑若干代后应当出现能命中的个体（分数 > 0）。
        use crate::plane::empty_mask;
        use crate::settle::Target;

        let mask = empty_mask();
        let targets = vec![Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 }];
        let ctx = ScoringContext {
            x0: 10.0,
            y0: 0.0,
            flip: true,
            step: 0.5,
            mask: &mask,
            targets: &targets,
            limit: 200,
        };
        let p = Params { generations: 6, population: 20, elite: 5, limit: 200 };
        let mut next = stream(0x1234_5678);
        let pop = initial_population(&p, &mut next);
        let out = evolve(pop, &p, &mut next, |t| fitness(t, &ctx));
        let best = out.first().expect("应有结果");
        assert!(
            best.score > 0.0,
            "6 代内应学会命中，最好的分数是 {}（函数 {} 个 token）",
            best.score,
            best.tokens.len()
        );
    }
}
