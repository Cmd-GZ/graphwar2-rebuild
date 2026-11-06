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

use crate::graphwar_level_generator::normal;
use crate::{Function, Token};

/// 权重总和，也就是二进制里 `random % 0x55` 的模数。
pub const TOTAL_WEIGHT: u32 = 85;

/// 常数 token 的标准差：`N(0,1) × 10.2`。
///
/// GW1 的 `RANDOM_FUNC_VALUE_MEAN` 同样是 10.2，两代一致。
pub const CONST_SCALE: f64 = 10.2;

/// 变量模式。**实测**（`docs/71` §5.1）：同一个分派表在两种模式下对"变量槽位"的处理不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarMode {
    /// 普通模式 `y = f(x)`：10 个变量权重全部给 `x`。
    Normal,
    /// 微分方程模式 `y' = f(x, y)`：拆成 `x` 6 个 + `y` 4 个。
    DiffEq,
}

/// `(tag, 权重)`，按分派表里的下标顺序排列。
///
/// 注意 `VarX` 的权重是 10 而不是 6：分派表里 11–16 走立即数分支、
/// 17–20 走"栈上预存值"分支，普通模式下两者都产出 `x`。
/// 运行时实测 `x` 占 11.77%（= 10/85），据此确认。
const TABLE_NORMAL: &[(Function, u32)] = &[
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

/// 微分方程模式：分派表 idx 11–16 给 `x`（6 个权重）、idx 17–20 给 `y`（4 个权重）。
const TABLE_DIFFEQ: &[(Function, u32)] = &[
    (Function::Const, 11),
    (Function::VarX, 6),
    (Function::VarY, 4),
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

/// 取当前模式的权重表。
pub fn table(mode: VarMode) -> &'static [(Function, u32)] {
    match mode {
        VarMode::Normal => TABLE_NORMAL,
        VarMode::DiffEq => TABLE_DIFFEQ,
    }
}

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
pub fn draw_tag(next: &mut dyn FnMut() -> u64, mode: VarMode) -> Function {
    let mut idx = (next() % u64::from(TOTAL_WEIGHT)) as u32;
    for &(tag, weight) in table(mode).iter() {
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
pub fn draw_token(next: &mut dyn FnMut() -> u64, mode: VarMode) -> Token {
    let tag = draw_tag(next, mode);
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
    /// **每个槽位的个体数**（反汇编 `cfg+0x78`）：1 个克隆 + 4 个工厂子代。
    ///
    /// 早期文档把它误标为"精英数 5"；`docs/95` 已证明精英是**比例**（每代保留前
    /// `种群/5` = 20% 个），而这个 5 是每槽个体数。
    pub per_slot: u32,
    /// 复杂度硬上限（`V > limit` 罚 5e7），六档 5/5/10/15/25/200。
    pub limit: u32,
}

/// 难度 1..=6 → 实测参数。越界按 6 处理。
///
/// ⚠️ 这是**运行时对某一台机器上六次求值**的实测组合（`docs/AI.md` §1），
/// 不是"关卡难度值 → 参数"的映射。真链路见 [`difficulty_params`]。
pub fn params(level: u8) -> Params {
    let (generations, population, limit) = match level {
        1 => (3, 50, 5),
        2 => (10, 100, 5),
        3 => (25, 150, 10),
        4 => (100, 200, 15),
        5 => (250, 250, 25),
        _ => (1000, 250, 200),
    };
    Params { generations, population, per_slot: 5, limit }
}

/// **关卡难度值 → GA 参数**（真链路）。`docs/147` §2 逐条给地址。
///
/// 难度值来自 `bot_added_event` 的载荷 `+0x10`（`0x4a0882`/`0x4a6d62` 两处写入，
/// 每关一个立即数：`3 / 25 / 50 / 100 / 250`），由 `0x3e3d75` 收到后**分档**：
///
/// ```text
/// 3e3d95  mov  ecx,0xa            ; s = 10
/// 3e3d9a  mov  r9d,0x5            ; limit = 5
/// 3e3da0  cmp  r14d,0x4
/// 3e3da4  jb   <写表>              ; d < 4   ⇒ (s=10, limit=5)
/// 3e3daa  mov  ecx,0x14           ; s = 20
/// 3e3daf  cmp  r14d,0xb
/// 3e3db3  jb   <写表>              ; d < 11  ⇒ (s=20, limit=5)
/// 3e3db9  cmp  r14d,0x1a
/// 3e3dbd  jae  3e456d             ; d >= 26 ⇒ 下面一段
/// 3e3dc3  mov  ecx,0x1e           ; s = 30
/// 3e3dc8  mov  r9d,0xa            ; limit = 10
/// 3e456d  cmp  r14d,0x65
/// 3e4571  jae  3e458d             ; d >= 101
/// 3e4573  mov  ecx,0x28           ; s = 40
/// 3e4578  mov  r9d,0xf            ; limit = 15
/// 3e458d  cmp  r14d,0xfb
/// 3e4594  mov  eax,0x19           ; 25
/// 3e4599  mov  r9d,0xc8           ; 200
/// 3e459f  cmovb r9d,eax           ; d < 251 ⇒ limit = 25，否则 200
/// 3e45a3  mov  ecx,0x32           ; s = 50
/// ```
///
/// 然后一次写表：`0x3e45cd call 0x478fc0(self, key, 难度, s, 5, limit)` ——
/// 注意第 3 个参数就是**难度值本身**，它在 `0x478fda` 起被原样搬进
/// `[rsp+0xe0]`，最终成为表项里的"代数"（`0x479ce5` 把 16 字节搬进 payload+0x70，
/// 驱动 `0x51b459` 再从那里读代数）。
///
/// ⇒ **难度值就是代数**；只有 `种群` 与 `复杂度上限` 按区间取档。
/// 这也解释了 `docs/118` §12.4 那个"关 12 的 `50` 对不上任何一列"：
/// `50` 落在 `26..=100` 那一档 ⇒ 代数 **50**、种群 **200**、上限 **15**，
/// 这个组合在六档表里不存在（六档表只测了 3/10/25/100/250/1000 这几个点）。
///
/// | 难度值 d | 代数 | s = 种群/5 | 种群 | 每槽 | 上限 |
/// |---|---|---|---|---|---|
/// | `0..=3` | d | 10 | 50 | 5 | 5 |
/// | `4..=10` | d | 20 | 100 | 5 | 5 |
/// | `11..=25` | d | 30 | 150 | 5 | 10 |
/// | `26..=100` | d | 40 | 200 | 5 | 15 |
/// | `101..=250` | d | 50 | 250 | 5 | 25 |
/// | `>=251` | d | 50 | 250 | 5 | 200 |
pub fn difficulty_params(difficulty: u32) -> Params {
    let (s, limit) = if difficulty < 4 {
        (10u32, 5u32)
    } else if difficulty < 11 {
        (20, 5)
    } else if difficulty < 26 {
        (30, 10)
    } else if difficulty < 101 {
        (40, 15)
    } else if difficulty < 251 {
        (50, 25)
    } else {
        (50, 200)
    };
    Params { generations: difficulty, population: s * 5, per_slot: 5, limit }
}

// ---------------------------------------------------------------------------
// 两阶段评分（`docs/97-two-phase-scoring.md`）
// ---------------------------------------------------------------------------
//
// 反汇编（vaddr；Ghidra = vaddr + 0x100000）：
//   * 每个个体先做**粗评** step=0.5（`0x51bc8b` / `0x51c0c1` 的 `movq 0x19f6d0`）；
//   * 代末排序 + 截断后，**前 3 名**再做**精评** step=0.01
//     （`0x51c3ab` / `0x51c6e6` / `0x51c9a0` 的 `movsd 0x19f110`，三段展开、非循环）；
//   * 精评与粗评"不一致"就把该条**整条换成新的随机函数**
//     （判据 `0x51c3b8`–`0x51c3e4`；换新 `0x4ef020`/`0x4ee0e0`/`0x4ee720`，从 `0x51c3ea` 起）。

/// 粗评步长。反汇编 `0x51bc8b` / `0x51c0c1` 读常量 `0x19f6d0` = 0.5。
pub const COARSE_STEP: f64 = 0.5;

/// 精评步长。反汇编 `0x51c3ab` / `0x51c6e6` / `0x51c9a0` 读常量 `0x19f110` = 0.01。
pub const FINE_STEP: f64 = 0.01;

/// 精评的条数 = **前 3 名**。
///
/// 反汇编里**没有**"3"这个立即数：是三段展开的代码，各自带 `len > rank` 的守卫
/// （`0x51c451 cmpq $0x0` / `0x51c78c cmpq $0x2` / `0x51ca46 cmpq $0x3`）。
/// 也没有配置字段控制它 —— 3 是硬编码。
pub const FINE_RANKS: usize = 3;

/// "一致"判定的距离容差 = 0.5。
///
/// 反汇编 `0x51c3dc ucomisd 0x19f6d0`：**与粗评步长复用同一个常量 0.5**。
pub const CONSISTENCY_TOL: f64 = 0.5;

/// 距离项未命中时的哨兵值。常量 `0x19fb30` = 1e6（`0x51ab35` / `0x51afe7` 写入）。
pub const DIST_SENTINEL: f64 = 1_000_000.0;

/// 评分阶段。两阶段**共用同一个评分核心**（`0x51a750`），只有 `step` 不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 粗评：种群里每一个个体都做一次。
    Coarse,
    /// 精评：只有代末前 3 名做。
    Fine,
}

impl Phase {
    /// 该阶段的采样步长（也正是评分核心的第 1 个 SSE 参数 `xmm0`）。
    pub const fn step(self) -> f64 {
        match self {
            Phase::Coarse => COARSE_STEP,
            Phase::Fine => FINE_STEP,
        }
    }
}

/// 一次评分的三个分量，逐字段对应评分核心的 sret 结构体
/// （写回点 `0x51b10d` / `0x51b11d` / `0x51b111` / `0x51b114`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evaluation {
    /// 排序键（sret `+0x00`）。= `hits*2e6 − dist − V²/100 − (V>limit ? 5e7 : 0)`。
    pub score: f64,
    /// 距离项 `d`（sret `+0x08`）：初值 [`DIST_SENTINEL`]，按事件取 min，被从分数里减掉。
    pub dist: f64,
    /// 命中数项（sret `+0x10`）：适应度里乘 2e6 的整数。
    pub hits: u32,
}

impl Evaluation {
    /// 只有分数的退化评价：`dist` 恒为哨兵、`hits` 恒为 0。
    ///
    /// 用它包装"与步长无关"的测试用适应度时，两阶段判据**永不触发**，
    /// 行为与改动前的单次评分完全一致（老测试就是这么过的）。
    pub const fn scalar(score: f64) -> Self {
        Self { score, dist: DIST_SENTINEL, hits: 0 }
    }

    /// 精评结果与粗评是否"一致"（= 保留该个体）。
    ///
    /// 逐条对应 `0x51c3b8`–`0x51c3e4`：
    ///
    /// ```text
    /// 51c3b8  mov  0x108(%rsp),%eax      ; eax = 精评 sret+0x10 = hits
    /// 51c3bf  cmp  0x38(%rsp),%eax       ; 与粗评存下的 hits（个体 +0x28）
    /// 51c3c3  jne  <换新>                 ; 1) 命中数不同 → 不一致
    /// 51c3c5  movsd 0x100(%rsp),%xmm0    ; xmm0 = 精评 sret+0x08 = dist
    /// 51c3ce  subsd 0x30(%rsp),%xmm0     ; − 粗评存下的 dist（个体 +0x20）
    /// 51c3d4  andpd <0x19c050>           ; |Δ|
    /// 51c3dc  ucomisd 0x19f6d0           ; 与 0.5 比较
    /// 51c3e4  jbe  <一致>                ; 2) |Δ| <= 0.5 → 一致
    /// ```
    ///
    /// 注意 `jbe`：`ucomisd` 遇到 NaN 会置 ZF/CF，因此 **NaN 也算"一致"**
    /// —— 这里用 `!(… > tol)` 而不是 `… <= tol` 来保持同样的语义。
    pub fn consistent_with(self, coarse: &Evaluation) -> bool {
        if self.hits != coarse.hits {
            return false;
        }
        !((self.dist - coarse.dist).abs() > CONSISTENCY_TOL)
    }
}

/// 把"只看 token、与阶段无关"的标量适应度包装成两阶段评分器。
///
/// 迁移用：老的 `FnMut(&[Token]) -> f64` 直接套一层即可继续喂给
/// [`next_generation`] / [`evolve`]，且两阶段判据永不触发（行为不变）。
pub fn scalar_scorer<F>(mut f: F) -> impl FnMut(&[Token], Phase) -> Evaluation
where
    F: FnMut(&[Token]) -> f64,
{
    move |tokens: &[Token], _phase: Phase| Evaluation::scalar(f(tokens))
}

/// 一个个体：token 序列 + 一次评分的三个分量。
///
/// 字段布局与二进制里 48 字节的个体逐一对应（`docs/97` §2）：
/// `Vec<Token>`（24 字节：ptr/len/cap），接着 `score`、`dist`、`hits`、`_`（再来 24 字节）。
#[derive(Debug, Clone, PartialEq)]
pub struct Individual {
    pub tokens: Vec<Token>,
    /// 排序键（个体 `+0x18` ← 评分结果 `+0x00`）。排序比较器读的就是这个字段。
    pub score: f64,
    /// 距离项（个体 `+0x20` ← 评分结果 `+0x08`）。
    pub dist: f64,
    /// 命中数项（个体 `+0x28` ← 评分结果 `+0x10`）。
    pub hits: u32,
}

impl Individual {
    /// 还没评过分的新个体（分数 0、距离项取哨兵、命中 0）。
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, score: 0.0, dist: DIST_SENTINEL, hits: 0 }
    }

    /// 带上一次评分结果的个体。
    pub fn with_evaluation(tokens: Vec<Token>, eval: Evaluation) -> Self {
        Self { tokens, score: eval.score, dist: eval.dist, hits: eval.hits }
    }

    /// 取出评分三元组（用于两阶段比对）。
    pub fn evaluation(&self) -> Evaluation {
        Evaluation { score: self.score, dist: self.dist, hits: self.hits }
    }

    /// 记下一次评分结果。
    pub fn set_evaluation(&mut self, eval: Evaluation) {
        self.score = eval.score;
        self.dist = eval.dist;
        self.hits = eval.hits;
    }
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
pub fn repair(tokens: &mut Vec<Token>, mode: VarMode, next: &mut dyn FnMut() -> u64) {
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
    need = 1;
    for t in tokens.iter() {
        need -= 1;
        need += arity(t.tag);
    }
    // 反汇编 FUN_005ee0e0（vaddr 0x3ee0e0）：补齐时 rand%50 决定追加什么。
    while need > 0 {
        let r = next() % 50;
        let tok = if r < 25 {
            Token::constant(normal(next) * CONST_SCALE)
        } else if r < 42 {
            Token::op(Function::VarX)
        } else {
            match mode {
                VarMode::Normal => Token::op(Function::VarX),
                VarMode::DiffEq => Token::op(Function::VarY),
            }
        };
        tokens.push(tok);
        need -= 1;
    }
}

/// 抽一个随机函数：先按 `trunc(N(0,1)×5+10)` 定长度，再抽那么多个 token，最后修复。
///
/// 长度公式与"抽多少发多少"都是**实测**（`docs/76`）；修复是**假设**（见 [`repair`]）。
pub fn random_function(next: &mut dyn FnMut() -> u64, mode: VarMode) -> Vec<Token> {
    let n = random_length(next).max(1) as usize;
    let mut tokens: Vec<Token> = (0..n).map(|_| draw_token(next, mode)).collect();
    // 补齐也必须用同一个 mode：Normal 补 x、DiffEq 补 y（反汇编 0x4ee2c4/0x4ee2ce）
    repair(&mut tokens, mode, next);
    tokens
}

/// 交叉：把两个父代的 token 序列各取一段拼起来，再修复。
///
/// 二进制里真正的交叉是 `FUN_003be7d0`（按 token 向量切片），
/// 这里的实现只保留"各取一段"这个**可观测特征**，具体切点规则**未还原**。
/// 工厂的"交叉"：从一个父代里取**一个完整子表达式**，再补齐。
///
/// 反汇编（`docs/99` §2）：游戏里工厂只有 `(out, &mode, &parent)` **一个父代**；
/// 取片段不是任意字节区间，而是按前缀式**递归取一棵完整子树**
/// （`FUN_004be7d0` vaddr `0x3be7d0`）。这里的做法：
///
/// ```text
/// r1 = rand % N           起点
/// r2 = rand % (N - r1)    长度上限
/// 从 r1 起按 arity 走，取到第一个"恰好补齐"的位置（不超过 r1+r2）
/// ```
pub fn cross_from(parent: &[Token], mode: VarMode, next: &mut dyn FnMut() -> u64) -> Vec<Token> {
    if parent.is_empty() {
        return Vec::new();
    }
    let r1 = (next() % parent.len() as u64) as usize;
    let r2 = (next() % (parent.len() - r1) as u64) as usize;
    let limit = r1 + r2.max(1);
    // 从前缀式的 r1 起，走到"子树完整"为止
    let mut need: i64 = 1;
    let mut end = r1;
    while end < parent.len() && end < limit {
        need -= 1;
        need += arity(parent[end].tag);
        end += 1;
        if need == 0 {
            break;
        }
    }
    if need != 0 {
        // 没走完一棵完整子树 → 退回"从 r1 到 limit"的裸片段，交给 repair 兜底
        end = limit.min(parent.len());
    }
    let mut out = parent[r1..end].to_vec();
    repair(&mut out, mode, next);
    out
}

/// 旧的两父代交叉，**保留但不再用于工厂**（游戏里没有两个父代，`docs/99` §2）。
/// 仅供测试与历史对照。
pub fn crossover(
    a: &[Token],
    b: &[Token],
    mode: VarMode,
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
    repair(&mut out, mode, next);
    out
}

/// 代末收尾：**前 3 名用 0.01 精评，不一致就整条换新**（反汇编 `0x51c3ab`–`0x51ca4f`）。
///
/// 三件必须照抄的行为（`docs/97` §3）：
///
/// 1. **不写回**：精评分数不覆盖个体的粗评分数 —— 一致时什么都不做
///    （`0x51c3e4 jbe 0x51c499`，只有释放临时 token 向量的代码），
///    不一致时也只覆写 token 向量那 24 字节（`0x51c480`–`0x51c495`），
///    `+0x18/+0x20/+0x28` 的旧粗评值留在原地。
/// 2. **整条换新**：新个体 = `makeRandomFunction → 构造/补齐 → 变异`
///    （`0x51c3f6` / `0x51c40a` / `0x51c44c`），与槽循环里"存活数不够"的随机分支
///    （`0x51b90f` / `0x51b923` / `0x51b95c`）是**同一套调用、同样的 RNG 消耗**。
/// 3. **换的是该名次本身**：第 0/1/2 名分别顶掉 `survivors[0]/[1]/[2]`
///    （写回点 `0x51c468`、`0x51c79b`、`0x51ca5d`，偏移 `+0x00/+0x30/+0x60`），
///    不是"丢掉一条再补一条到队尾"。
fn fine_pass<F>(
    survivors: &mut [Individual],
    mode: VarMode,
    next: &mut dyn FnMut() -> u64,
    score: &mut F,
) where
    F: FnMut(&[Token], Phase) -> Evaluation,
{
    for rank in 0..FINE_RANKS.min(survivors.len()) {
        // 反汇编：三名各来一次 `call 0x51a750`，xmm0 = 0.01
        let coarse = survivors[rank].evaluation();
        let fine = score(&survivors[rank].tokens, Phase::Fine);
        if fine.consistent_with(&coarse) {
            continue; // 一致 → 保留粗评结果，什么都不写
        }
        // 不一致 → 整条换新。`random_function` 内含 0x4ef020 与 0x4ee0e0 两步。
        let mut tokens = random_function(next, mode);
        mutate(&mut tokens, next); // 0x4ee720
        survivors[rank].tokens = tokens;
        // 注意：分数字段保持旧值（反汇编只覆盖 Vec 的 ptr/len/cap）。
    }
}

/// 走一代。
///
/// 与二进制 `FUN_0061b440` 的代末顺序一致（`docs/95`、`docs/97`）：
///
/// ```text
/// 槽 i ∈ [0, s)：1 个克隆重评（0x51bc8b @0.5）+ (per_slot−1) 个工厂子代（0x51c0c1 @0.5）
/// ⇒ 共 s × per_slot = 种群 个个体，每个都做一次粗评
/// 降序排序（0x51cb6f）→ 截断到前 s 个（0x51c208）
/// 前 3 名精评 @0.01（0x51c3b3 / 0x51c6ee / 0x51c9a8），不一致就整条换新
/// ```
///
/// **返回值是 `s = 种群/per_slot` 个存活者（已排序）**，不是整代 `种群` 个
/// —— 二进制里截断之后数组就只剩 s 个，下一代的槽循环按槽号取 `prev[0..s]`。
/// （改动前这里返回整代 `种群` 个，是本次修正的一部分，见 `docs/97` §6。）
///
/// ⚠️ 仍是假设/未建模的三条（与本次改动无关）：
/// ① 工厂内部的交叉/重生细节见 [`crossover`]；
/// ② `pop.len() < s` 时二进制会走"随机新生"分支（`0x51b82c jbe 0x51b900`，
///    那是**第 0 代**的初始化路径）；这里假设调用方给的种群够长
///    （[`initial_population`] 保证 `len = 种群`），不够时退化为重复最后一条；
/// ③ 初始种群的生成方式仍未完全还原（`docs/95` §5 未定）。
pub fn next_generation<F>(
    pop: &[Individual],
    p: &Params,
    mode: VarMode,
    next: &mut dyn FnMut() -> u64,
    mut score: F,
) -> Vec<Individual>
where
    F: FnMut(&[Token], Phase) -> Evaluation,
{
    let mut sorted: Vec<Individual> = pop.to_vec();
    sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    if sorted.is_empty() {
        return Vec::new();
    }

    let n = p.population as usize;
    let per_slot = p.per_slot.max(1) as usize;
    let survivors = n / per_slot; // s = 种群 / 每槽个体数 = 20%
    let mut out: Vec<Individual> = Vec::with_capacity(n);

    for i in 0..survivors {
        // ① 克隆头部第 i 个（索引顺序，实测），并**重新粗评**
        //    （反汇编 0x51b8c5 memcpy → 0x51b8fa → 0x51bc8b call 0x51a750 @0.5）
        let mut clone = sorted[i.min(sorted.len() - 1)].clone();
        clone.set_evaluation(score(&clone.tokens, Phase::Coarse));
        out.push(clone);
        // ② per_slot − 1 个工厂子代，**全部共用同一个父代** = 本槽的 sorted[i]
        //    （反汇编 0x51bdd7 折叠 → 0x51be5c 拷一份 → 0x51be93 call 0x4ee3a0，
        //     后者只有 (out, &mode, &parent) 三个参数 ⇒ 游戏里**没有"两个父代"**；
        //     见 docs/99 §2）
        for _ in 1..per_slot {
            let parent = &sorted[i.min(sorted.len() - 1)];
            let tokens = if parent.tokens.len() < 20 {
                // 父代太短 → 重生（实测 cmp $0x14）
                random_function(next, mode)
            } else {
                let mut child = cross_from(&parent.tokens, mode, next);
                mutate(&mut child, next);
                child
            };
            // 反汇编 0x51c0c1：工厂子代那一份粗评（同一循环体被复制成两块）
            let eval = score(&tokens, Phase::Coarse);
            out.push(Individual::with_evaluation(tokens, eval));
        }
    }
    out.truncate(n);

    // 代末：降序排序 + 只留前 s 个（0x51c1e2 → 0x51cb6f 分派 → 0x51c208 截断）
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(survivors);

    // 前 3 名精评 + 纠偏（0x51c3ab 起）
    fine_pass(&mut out, mode, next, &mut score);
    out
}

/// 跑满 `params.generations` 代，返回**存活者**（`种群/per_slot` 个，按分数降序）。
///
/// 这就是"一回合思考"的骨架：调用方负责提供初始种群与两阶段评分器。
pub fn evolve<F>(
    pop: Vec<Individual>,
    p: &Params,
    mode: VarMode,
    next: &mut dyn FnMut() -> u64,
    score: F,
) -> Vec<Individual>
where
    F: FnMut(&[Token], Phase) -> Evaluation,
{
    evolve_with_budget(pop, p, mode, next, score, None)
}

/// 真游戏的每回合墙钟上限，单位秒（反汇编 `0x51b7cf`：`cmp $0x19`）。
///
/// 终止条件是「代数到顶 ∨ elapsed > 25 s」取先到，**两者共用同一出口**，
/// 所以调用方无法得知是否被截断（`docs/93`）。难度 6（1000 代）在实际机器上
/// 通常会撞到它：实测期望 253000 次评分，只跑到 217580 / 234278。
pub const GAME_TIME_BUDGET_SECS: f64 = 25.0;

/// 带墙钟预算的进化。`max_secs = None` 表示不限时（测试用，保证可复现）。
pub fn evolve_with_budget<F>(
    mut pop: Vec<Individual>,
    p: &Params,
    mode: VarMode,
    next: &mut dyn FnMut() -> u64,
    mut score: F,
    max_secs: Option<f64>,
) -> Vec<Individual>
where
    F: FnMut(&[Token], Phase) -> Evaluation,
{
    for ind in pop.iter_mut() {
        if ind.score == 0.0 {
            let eval = score(&ind.tokens, Phase::Coarse);
            ind.set_evaluation(eval);
        }
    }
    let start = std::time::Instant::now();
    for _ in 0..p.generations {
        // 与游戏一致：**每代顶部**检查一次（不是每次评分都查）
        if let Some(limit) = max_secs {
            if start.elapsed().as_secs_f64() > limit {
                break;
            }
        }
        pop = next_generation(&pop, p, mode, next, &mut score);
    }
    pop
}

/// 造一个初始种群。
pub fn initial_population(p: &Params, mode: VarMode, next: &mut dyn FnMut() -> u64) -> Vec<Individual> {
    (0..p.population)
        .map(|_| Individual::new(random_function(next, mode)))
        .collect()
}

// ---------------------------------------------------------------------------
// 按士兵的进化层（docs/98 §3 / §7 第 3 条）
// ---------------------------------------------------------------------------
//
// 真链路（`docs/98` §0/§3/§4）：
//
// ```text
// turn_manager 0x463da0   选好本回合的 soldier_id，发 TurnInfo
//   ↓（computer_player_manager 0x4785f0 / 0x47a300）
// AI 入口 0x47a300        为**这一个**士兵跑一次进化（变体 A 0x51b440）
//   ↓
// 0x3f27d0                结果写回共享表
//   ↓
// FunctionInfo{owner_id = soldier_id}
// ```
//
// 三条硬结论（全部有 replay 观测）：
//
// 1. **AI 不选射手**。`soldier_id` 由 [`crate::graphwar_turn_manager::TurnManager`] 先定，本层只接收它；
// 2. **槽按士兵**：`FunctionInfo.owner_id` / `FunctionActive.owner_id` 就是 `soldier_id`，
//    开局每个士兵各收一条**空**函数（`docs/98` §3.1）；
// 3. **每回合重算**：同一士兵**下一次**被轮到时文本又变了（`docs/98` §3.2 的 tick
//    168 / 12269 / 26038 三行）⇒ 写回 = 覆盖，不是"算一次缓存住"。
//
// 因此"同一玩家的两个士兵"各有各的槽，**不共享**；本模块从类型上保证这一点
// （[`SoldierFunctions`] 是 `soldier_id → 槽` 的映射，没有"玩家 → 槽"这一层）。
//
// ⚠️ **仍未确定**（`docs/98` §6-A）：`computer_player_manager` 的
// `HashMap<i32, V>` 键是 `player_id` 还是 `soldier_id`——也就是"**进化的种群**是每人一份
// 还是每士兵一份"。本层因此**不持有种群**：种群由调用方传进来、由调用方决定下一回合
// 复用给谁（[`SoldierTurn::survivors`] 就是这份"可以传给下一次的种子"）。

/// 一个士兵的函数槽。
///
/// 对应协议里的 `FunctionInfo{owner_id, function, corrected_function}`：**一个士兵一格**。
/// 槽里存的是**前缀 token 序列**，不是文本——token → 文本（`0x4ec160`）还没实现，
/// 见 `docs/109` §"仍未确定"。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FunctionSlot {
    /// `owner_id` = `soldier_id`（`docs/98` §3.1）。
    pub owner_id: i64,
    /// 这个士兵当前的函数（前缀 token）。空 = 开局那条空 `FunctionInfo`。
    pub tokens: Vec<Token>,
    /// 这个槽最后一次被写入的回合号（`None` = 开局以来还没进化过）。
    pub updated_turn: Option<u64>,
}

impl FunctionSlot {
    pub fn new(owner_id: i64) -> Self {
        Self { owner_id, tokens: Vec::new(), updated_turn: None }
    }

    /// 开局状态：空函数、没被算过（`docs/98` §3.1 的 tick=15 那批空 `FunctionInfo`）。
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

/// `soldier_id → 函数槽`。**一士兵一格**，同一玩家的不同士兵互不影响（`docs/98` §3.3）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SoldierFunctions {
    slots: std::collections::BTreeMap<i64, FunctionSlot>,
}

impl SoldierFunctions {
    pub fn new() -> Self {
        Self { slots: std::collections::BTreeMap::new() }
    }

    /// 登记一个士兵的**空**槽（开局每个士兵各一条空 `FunctionInfo`）。
    ///
    /// 已存在时**不覆盖**（重复登记不是"把这格清空"）。
    pub fn register(&mut self, soldier_id: i64) -> &mut FunctionSlot {
        self.slots.entry(soldier_id).or_insert_with(|| FunctionSlot::new(soldier_id))
    }

    /// 这个士兵的槽（没登记过 = `None`）。
    pub fn slot(&self, soldier_id: i64) -> Option<&FunctionSlot> {
        self.slots.get(&soldier_id)
    }

    /// 这个士兵当前的函数。
    pub fn tokens(&self, soldier_id: i64) -> Option<&[Token]> {
        self.slots.get(&soldier_id).map(|s| s.tokens.as_slice())
    }

    /// 写回一个士兵的函数（`0x3f27d0` 的 Rust 形态）。**覆盖**，不是追加。
    ///
    /// 没登记过的士兵会被顺手建格（真实链路里槽是开局建的，这里只是不 panic）。
    pub fn set(&mut self, soldier_id: i64, tokens: Vec<Token>, turn: u64) -> &mut FunctionSlot {
        let slot = self.slots.entry(soldier_id).or_insert_with(|| FunctionSlot::new(soldier_id));
        slot.tokens = tokens;
        slot.updated_turn = Some(turn);
        slot
    }

    /// 这个士兵被算过的回合号。
    pub fn updated_turn(&self, soldier_id: i64) -> Option<u64> {
        self.slots.get(&soldier_id).and_then(|s| s.updated_turn)
    }

    /// 已登记的士兵数。
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// 已登记的士兵 id（`BTreeMap` ⇒ 顺序确定）。
    pub fn soldier_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.slots.keys().copied()
    }
}

/// 一个士兵"这一回合"的进化结果。
///
/// `tokens` 已经写进 [`SoldierFunctions`] 里该士兵的槽；`survivors` 是这一代留下的
/// 存活者（按分数降序），交给调用方决定下一回合当谁的初始种群（§6-A 未定）。
#[derive(Debug, Clone, PartialEq)]
pub struct SoldierTurn {
    pub soldier_id: i64,
    /// 这是该士兵第几回合被轮到时算的（写入 [`FunctionSlot::updated_turn`]）。
    pub turn: u64,
    /// 本回合该士兵要发的函数（写进 `FunctionInfo{soldier_id}` 的那条）。
    pub tokens: Vec<Token>,
    /// 进化后的存活者，按分数降序（`evolve_with_budget` 的返回）。
    pub survivors: Vec<Individual>,
}

/// "为一个士兵进化一次"的入口。
///
/// 它是 [`evolve_with_budget`] 外面的一层皮，负责**上下文绑定**：
/// 评分上下文由"本回合这个射手"的位置/朝向决定（`flip` 来自队伍，`docs/105`），
/// 参数与预算来自难度档（`docs/93`），结果写进这个士兵自己的槽。
///
/// ```text
/// // 典型调用顺序（`graphwar_turn_manager.rs` 先定射手，AI 只为它跑）：
/// let d = turns.begin_turn(player_id, |id| life_state(id), timed_out);
/// if let TurnDecision::Announce { soldier_id, flip, .. } = d {
///     if soldier_id != 0 {
///         let ctx = ScoringContext { x0, y0, flip, step: SERVER_STEP, mask, targets, limit };
///         let ev = SoldierEvolver::new(params(level), VarMode::Normal, ctx)
///             .with_budget(Some(GAME_TIME_BUDGET_SECS));
///         let out = ev.evolve_soldier(&mut slots, soldier_id, turn_no, pop, &mut next);
///     }
/// }
/// ```
pub struct SoldierEvolver<'a> {
    /// 难度参数（`params(level)`）。
    pub params: Params,
    /// 变量模式（`Normal` / `DiffEq`，见 `docs/94`）。
    pub mode: VarMode,
    /// 这一回合的评分上下文（射手的 `x0/y0/flip` + 掩膜 + 目标）。
    pub ctx: ScoringContext<'a>,
    /// 墙钟预算：`Some(25.0)` = 真游戏（`docs/93`）；`None` = 不限时（测试用）。
    pub budget_secs: Option<f64>,
}

impl<'a> SoldierEvolver<'a> {
    pub fn new(params: Params, mode: VarMode, ctx: ScoringContext<'a>) -> Self {
        Self { params, mode, ctx, budget_secs: None }
    }

    /// 设墙钟预算。真游戏用 `Some(GAME_TIME_BUDGET_SECS)`；测试传 `None` 保证可复现。
    pub fn with_budget(mut self, secs: Option<f64>) -> Self {
        self.budget_secs = secs;
        self
    }

    /// 造初始种群（第 0 代）。真实游戏的 gen-0 生成路径**仍未还原**（`docs/95` §5、
    /// `HANDOVER` §7.11.1），这里用 [`initial_population`]，与旧行为一致。
    pub fn seed_population(&self, next: &mut dyn FnMut() -> u64) -> Vec<Individual> {
        initial_population(&self.params, self.mode, next)
    }

    /// 为**一个** `soldier_id` 跑一次进化，并把结果写进**它自己的**槽。
    ///
    /// * 不选射手：`soldier_id` 必须由回合系统先给出（`docs/98` §0）；
    /// * 不共享：同一玩家的另一个士兵的槽**完全不动**；
    /// * 每回合重算：同一士兵第二次调用会**覆盖**上一次的 `tokens`，`updated_turn` 前进。
    ///
    /// 边界：`pop` 为空时不产生存活者，槽会被写成**空函数**（`updated_turn` 仍前进）。
    /// 真实链路里种群至少有 1 个个体（[`initial_population`] 保证），这里只是不 panic。
    pub fn evolve_soldier(
        &self,
        slots: &mut SoldierFunctions,
        soldier_id: i64,
        turn: u64,
        pop: Vec<Individual>,
        next: &mut dyn FnMut() -> u64,
    ) -> SoldierTurn {
        let ctx = &self.ctx;
        let mut score = |tokens: &[Token], phase: Phase| ctx.evaluate(tokens, phase);
        let survivors = evolve_with_budget(pop, &self.params, self.mode, next, &mut score, self.budget_secs);
        // evolve/next_generation 的返回是"按分数降序"的存活者，第 0 个就是本回合要发的函数。
        let tokens = survivors.first().map(|ind| ind.tokens.clone()).unwrap_or_default();
        slots.set(soldier_id, tokens.clone(), turn);
        SoldierTurn { soldier_id, turn, tokens, survivors }
    }
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
    pub targets: &'a [crate::graphwar_shot_calculator::settle::Target],
    /// 复杂度硬上限（`V > limit` 罚 5e7）。
    pub limit: u32,
}

impl<'a> ScoringContext<'a> {
    /// 同一上下文换个采样步长。
    ///
    /// 两阶段评分**只差这一个字段**（反汇编里就是评分核心第 1 个 SSE 参数
    /// `xmm0`：粗评 0.5、精评 0.01），所以换步长不需要重建上下文。
    pub fn with_step(&self, step: f64) -> ScoringContext<'a> {
        ScoringContext {
            x0: self.x0,
            y0: self.y0,
            flip: self.flip,
            step,
            mask: self.mask,
            targets: self.targets,
            limit: self.limit,
        }
    }

    /// 按阶段评分：`Phase::Coarse` → 0.5、`Phase::Fine` → 0.01。
    ///
    /// 直接当两阶段评分器用：`|tokens, phase| ctx.evaluate(tokens, phase)`。
    pub fn evaluate(&self, tokens: &[Token], phase: Phase) -> Evaluation {
        let ctx = self.with_step(phase.step());
        fitness_eval(tokens, &ctx)
    }
}

/// 适应度：`命中×2e6 − dist − V²/100 − (V > limit ? 5e7 : 0)`。
///
/// **实测**（`docs/74`）：常规对局里没有"友军惩罚"项。
/// `V` 就是 token 数；`hits` 是这次弹道判死的士兵数。
///
/// 返回值只有分数 —— 两阶段判据另见 [`fitness_eval`]。
pub fn fitness(tokens: &[Token], ctx: &ScoringContext) -> f64 {
    fitness_eval(tokens, ctx).score
}

/// 适应度的**完整**结果：分数 + 距离项 `d` + 命中数项。
///
/// 三个分量正是评分核心 sret 结构体的前 24 字节，也是两阶段判据要比的东西
/// （`docs/97` §3）。`dist` 的语义见 [`Evaluation::dist`]：
///
/// * 初值 [`DIST_SENTINEL`]（1e6，`0x19fb30`）；
/// * 命中时按"命中点到目标圆心的距离"取 **min**（反汇编 `0x51aed6` 的
///   `ucomisd` + `jbe` 就是取 min），所以 `dist ∈ [0, radius]`；
/// * 分数里是**减**它（`0x51b0d9 subsd 0x8(%rsp)`），未命中时`−1e6` 就是这么来的。
///
/// ⚠️ 与原版的差距（**推断**）：`docs/74` 的运行实测显示常规对局里 `d` 恒为 1e6
/// （全未命中约 −1e6、命中一人约 +1e6），也就是说更新分支在原版里通常不触发。
/// 我们这里如实计算命中点距离，因此距离判据**比原版更活跃**（但仍受
/// `dist ∈ [0, radius]` 约束，radius=0.5 时 `|Δ| <= 0.5` 恒成立 ⇒ 实际仍然不触发）。
pub fn fitness_eval(tokens: &[Token], ctx: &ScoringContext) -> Evaluation {
    use crate::graphwar_path_calculator::{calculate, Mode};
    use crate::plane::is_occupied;
    use crate::graphwar_shot_calculator::{correction, shifted_tokens};

    let v = tokens.len();
    let mut score = -1_000_000.0 - (v * v) as f64 / 100.0;
    if v as u32 > ctx.limit {
        score -= 50_000_000.0;
    }
    if tokens.is_empty() {
        return Evaluation { score, dist: DIST_SENTINEL, hits: 0 };
    }

    // 修正后沿弹道走一遍：先判致死（穿过即死、不挡弹道），再判障碍阻挡
    let c = correction(tokens, ctx.x0, ctx.y0, ctx.flip);
    let shifted = shifted_tokens(tokens, c);
    let points = calculate(&shifted, ctx.x0, ctx.y0, ctx.step, ctx.flip, Mode::Normal);

    let mut killed: Vec<i64> = Vec::new();
    let mut dist = DIST_SENTINEL;
    for &(px, py) in &points {
        for t in ctx.targets {
            if killed.contains(&t.id) {
                continue;
            }
            let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
            if d <= t.radius {
                killed.push(t.id);
                if d < dist {
                    dist = d;
                }
            }
        }
        if is_occupied(ctx.mask, px, py) {
            break;
        }
    }
    Evaluation {
        score: score + killed.len() as f64 * 2_000_000.0,
        dist,
        hits: killed.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_scaffold::rng::ChaCha12;

    fn stream(seed: u64) -> impl FnMut() -> u64 {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        let mut r = ChaCha12::from_seed(&key);
        move || r.next_u64()
    }

    #[test]
    fn weights_sum_to_85() {
        for mode in [VarMode::Normal, VarMode::DiffEq] {
            let sum: u32 = table(mode).iter().map(|(_, w)| *w).sum();
            assert_eq!(sum, TOTAL_WEIGHT, "{mode:?} 的权重之和必须等于模数 85");
        }
        assert_eq!(table(VarMode::Normal).len(), 15, "普通模式 15 个非零区间");
        assert_eq!(table(VarMode::DiffEq).len(), 16, "DiffEq 模式把变量槽拆成 x/y 两段");
        // DiffEq 下变量权重拆成 x=6 / y=4（实测 6.83% / 4.86%，docs/71 §5.1）
        let d = table(VarMode::DiffEq);
        assert_eq!(d.iter().find(|(t, _)| *t == Function::VarX).unwrap().1, 6);
        assert_eq!(d.iter().find(|(t, _)| *t == Function::VarY).unwrap().1, 4);
    }

    #[test]
    fn distribution_matches_table() {
        let mut next = stream(0x9E37_79B9_7F4A_7C15);
        const N: u32 = 170_000; // 2000 轮 × 85
        let mut got = [0u32; 20];
        for _ in 0..N {
            got[draw_tag(&mut next, VarMode::Normal) as usize] += 1;
        }
        for &(tag, weight) in table(VarMode::Normal).iter() {
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
            let t = draw_token(&mut next, VarMode::Normal);
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
            let tag = draw_tag(&mut next, VarMode::Normal);
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
                d.push(draw_token(&mut || r.next_u64(), VarMode::Normal));
            }
            repair(&mut d, VarMode::Normal, &mut next);
            assert!(is_well_formed(&d), "修复后应合法：{d:?}");
        }
    }

    #[test]
    fn random_function_is_always_well_formed() {
        let mut next = stream(0xBEEF);
        for _ in 0..500 {
            let f = random_function(&mut next, VarMode::Normal);
            assert!(!f.is_empty(), "不应生成空函数");
            assert!(is_well_formed(&f), "生成的函数应合法：{f:?}");
        }
    }

    #[test]
    fn params_match_measured_table() {
        assert_eq!(params(1), Params { generations: 3, population: 50, per_slot: 5, limit: 5 });
        assert_eq!(params(3), Params { generations: 25, population: 150, per_slot: 5, limit: 10 });
        assert_eq!(params(6), Params { generations: 1000, population: 250, per_slot: 5, limit: 200 });
    }

    #[test]
    fn next_generation_keeps_size_and_reuses_head() {
        let p = params(1); // 种群 50 → 存活 s = 10，每个槽 1 克隆 + 4 工厂子代
        let mut next = stream(0xFEED);
        let mut pop = initial_population(&p, VarMode::Normal, &mut next);
        for (i, ind) in pop.iter_mut().enumerate() {
            ind.score = i as f64;
        }
        let head: Vec<Vec<Token>> = {
            let mut s = pop.clone();
            s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            s.iter().take(10).map(|i| i.tokens.clone()).collect()
        };
        // 记录评分调用顺序：每槽 = 1 个克隆 + 4 个工厂子代 ⇒ 第 5k 次就是第 k 个克隆。
        let mut seen: Vec<Vec<Token>> = Vec::new();
        let mut score = |t: &[Token], _phase: Phase| {
            seen.push(t.to_vec());
            Evaluation::scalar(t.len() as f64)
        };
        let g1 = next_generation(&pop, &p, VarMode::Normal, &mut next, &mut score);
        // 改动后：返回值是**存活者** s = 种群/per_slot（二进制里截断后数组就只剩 s 个）
        assert_eq!(g1.len(), (p.population / p.per_slot) as usize, "应返回 s = 种群/5 个存活者");
        // 槽 i 的克隆就是上一代排好序的第 i 个（docs/95 §2.1：按槽号递增取 prev[0], prev[1], …）
        for (k, t) in head.iter().enumerate() {
            assert_eq!(&seen[k * p.per_slot as usize], t, "第 {k} 个槽的克隆应是头部第 {k} 个");
        }
        // 克隆必须**重新粗评**（不是沿用父代分数）
        assert!(g1.iter().any(|i| i.score != 0.0), "克隆/子代都应带着自己的评分");
    }

    #[test]
    fn evolve_runs_requested_generations() {
        let p = Params { generations: 3, population: 10, per_slot: 5, limit: 200 };
        let mut next = stream(99);
        let pop = initial_population(&p, VarMode::Normal, &mut next);
        let out = evolve(
            pop,
            &p,
            VarMode::Normal,
            &mut next,
            scalar_scorer(|t: &[Token]| t.len() as f64),
        );
        assert_eq!(out.len(), (p.population / p.per_slot) as usize);
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
        use crate::graphwar_shot_calculator::settle::Target;

        let mask = empty_mask();
        let targets = vec![Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 }];
        let ctx = ScoringContext {
            x0: 10.0,
            y0: 0.0,
            flip: true,
            step: crate::SERVER_STEP,
            mask: &mask,
            targets: &targets,
            limit: 200,
        };
        // 现在每代只留 20% 存活者（s = 种群/5），所以种群要比以前大一些。
        let p = Params { generations: 8, population: 50, per_slot: 5, limit: 200 };
        let mut next = stream(0x1234_5678);
        let pop = initial_population(&p, VarMode::Normal, &mut next);
        // 两阶段：粗评 0.5、精评 0.01（后者只跑前 3 名）
        let mut score = |t: &[Token], phase: Phase| ctx.evaluate(t, phase);
        let out = evolve(pop, &p, VarMode::Normal, &mut next, &mut score);
        let best = out.first().expect("应有结果");
        assert!(
            best.score > 0.0,
            "8 代内应学会命中，最好的分数是 {}（函数 {} 个 token）",
            best.score,
            best.tokens.len()
        );
    }

    // -----------------------------------------------------------------------
    // 两阶段评分（docs/97）
    // -----------------------------------------------------------------------

    #[test]
    fn two_phase_constants_match_disassembly() {
        assert_eq!(COARSE_STEP, 0.5); // 0x19f6d0
        assert_eq!(FINE_STEP, 0.01); // 0x19f110
        assert_eq!(Phase::Coarse.step(), 0.5);
        assert_eq!(Phase::Fine.step(), 0.01);
        assert_eq!(FINE_RANKS, 3); // 三段展开
        assert_eq!(CONSISTENCY_TOL, 0.5); // 与粗评步长复用同一常量
        assert_eq!(DIST_SENTINEL, 1_000_000.0); // 0x19fb30
    }

    #[test]
    fn consistency_rule_matches_the_asm() {
        let base = Evaluation { score: 0.0, dist: 1_000_000.0, hits: 1 };
        // 1) 命中数不同 → 不一致（0x51c3c3 jne）
        assert!(!Evaluation { score: 1.0, dist: 1_000_000.0, hits: 2 }.consistent_with(&base));
        assert!(!Evaluation { score: -1.0, dist: 1_000_000.0, hits: 0 }.consistent_with(&base));
        // 2) 命中数相同、|Δdist| <= 0.5 → 一致（0x51c3e4 jbe）
        assert!(Evaluation { score: 9.0, dist: 1_000_000.5, hits: 1 }.consistent_with(&base));
        assert!(Evaluation { score: 9.0, dist: 999_999.5, hits: 1 }.consistent_with(&base));
        // 3) |Δdist| > 0.5 → 不一致
        assert!(!Evaluation { score: 9.0, dist: 1_000_000.6, hits: 1 }.consistent_with(&base));
        assert!(!Evaluation { score: 9.0, dist: 999_999.4, hits: 1 }.consistent_with(&base));
        // 4) NaN 走 ucomisd 的 unordered ⇒ CF/ZF 置位 ⇒ jbe 成立 ⇒ 算"一致"
        assert!(Evaluation { score: 9.0, dist: f64::NAN, hits: 1 }.consistent_with(&base));
    }

    /// 代末前 3 名精评：**粗评看不到、精评看得见的东西** 才会触发换新。
    ///
    /// 场景：射手 (10,0) → 目标在原点，中间一堵很薄的竖墙（x≈1.25、厚 2 px = 0.1 单位）。
    /// 粗评 step=0.5 的采样点落在 x=1.5 与 x=1.0 上、**跨过**这堵墙 ⇒ 以为能命中；
    /// 精评 step=0.01 一定踩进墙里 ⇒ 知道打不到。这正是"精评纠偏"。
    #[test]
    fn fine_pass_replaces_individuals_when_hits_disagree() {
        use crate::plane::{empty_mask, is_occupied, to_pixel};

        let mut mask = empty_mask();
        // 一点点试出墙的位置：要求粗评（0.5）的采样都躲开、精评（0.01）踩中
        for px in 522..=528 {
            for py in 200..400 {
                mask[py * 1000 + px] = 0x00;
            }
        }
        // 自检：粗评的两条采样点不在墙上，精评的一定会踩到
        assert!(!is_occupied(&mask, 1.5, 0.0), "粗评采样点 x=1.5 应在墙外");
        assert!(!is_occupied(&mask, 1.0, 0.0), "粗评采样点 x=1.0 应在墙外");
        assert!(is_occupied(&mask, 1.25, 0.0), "精评应踩到墙");
        assert_eq!(to_pixel(1.25, 0.0), (525, 300));

        use crate::graphwar_shot_calculator::settle::Target;
        let targets = vec![Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 }];
        let ctx = ScoringContext {
            x0: 10.0,
            y0: 0.0,
            flip: true,
            step: crate::SERVER_STEP,
            mask: &mask,
            targets: &targets,
            limit: 200,
        };
        // 一条"直着打过去"的函数：y = 0（前缀式只有一个常数 token）
        let straight = vec![Token::constant(0.0)];
        let coarse = ctx.evaluate(&straight, Phase::Coarse);
        let fine = ctx.evaluate(&straight, Phase::Fine);
        assert_eq!(coarse.hits, 1, "粗评跨过薄墙，以为命中");
        assert_eq!(fine.hits, 0, "精评踩到墙，知道打不到");
        assert!(!fine.consistent_with(&coarse), "命中数不同 ⇒ 不一致");

        // 让这条函数成为第一名，其余个体都是"很差"的函数
        let mut p = Params { generations: 1, population: 5, per_slot: 5, limit: 200 };
        p.population = 5; // s = 1：只有一个槽，正好第 0 名
        let pop = vec![Individual::with_evaluation(straight.clone(), coarse)];
        let mut next = stream(0x51C3B3);
        let out = next_generation(
            &pop,
            &p,
            VarMode::Normal,
            &mut next,
            |t: &[Token], phase| ctx.evaluate(t, phase),
        );
        assert_eq!(out.len(), 1, "s = 种群/5 = 1");
        assert_ne!(out[0].tokens, straight, "不一致 ⇒ 该名次被换成新随机函数");
        // 分数**不写回**：仍是粗评（旧）值（反汇编 0x51c480 只覆盖 Vec）
        assert_eq!(out[0].score, coarse.score, "换新后仍保留旧粗评分数");
    }

    #[test]
    fn fine_pass_keeps_individual_when_consistent() {
        use crate::plane::empty_mask;
        use crate::graphwar_shot_calculator::settle::Target;

        // 空场：粗评、精评都说命中 ⇒ 一致 ⇒ 什么都不换
        let mask = empty_mask();
        let targets = vec![Target { id: 1, x: 0.0, y: 0.0, radius: 0.5 }];
        let ctx = ScoringContext {
            x0: 10.0,
            y0: 0.0,
            flip: true,
            step: crate::SERVER_STEP,
            mask: &mask,
            targets: &targets,
            limit: 200,
        };
        let straight = vec![Token::constant(0.0)];
        let coarse = ctx.evaluate(&straight, Phase::Coarse);
        assert_eq!(coarse.hits, 1);
        let p = Params { generations: 1, population: 5, per_slot: 5, limit: 200 };
        let pop = vec![Individual::with_evaluation(straight.clone(), coarse)];
        let mut next = stream(7);
        let out = next_generation(
            &pop,
            &p,
            VarMode::Normal,
            &mut next,
            |t: &[Token], phase| ctx.evaluate(t, phase),
        );
        assert_eq!(out[0].tokens, straight, "一致 ⇒ 函数原样保留");
        assert_eq!(out[0].score, coarse.score, "一致 ⇒ 分数保持粗评");
    }

    #[test]
    fn each_generation_coarsely_scores_everyone_plus_three_fine() {
        // 反汇编 `0x51bc8b`（克隆）/`0x51c0c1`（工厂子代）都在 0.5 上评分，
        // 代末另有 3 次 0.01 精评 ⇒ 每代 (种群 + 3) 次。
        // 这与 docs/67 的运行时实测"每回合 = 代数 × (种群 + 3)"一致。
        let p = Params { generations: 3, population: 20, per_slot: 5, limit: 200 };
        let mut next = stream(0xC0DE);
        let mut pop = initial_population(&p, VarMode::Normal, &mut next);
        for (i, ind) in pop.iter_mut().enumerate() {
            ind.score = i as f64; // 让排序确定：分数 = 下标
        }
        let (mut coarse_n, mut fine_n) = (0usize, 0usize);
        let mut score = |t: &[Token], phase: Phase| {
            match phase {
                Phase::Coarse => coarse_n += 1,
                Phase::Fine => fine_n += 1,
            }
            Evaluation::scalar(t.len() as f64)
        };
        let _ = next_generation(&pop, &p, VarMode::Normal, &mut next, &mut score);
        assert_eq!(coarse_n, p.population as usize, "每个个体（含克隆）都要粗评");
        assert_eq!(fine_n, FINE_RANKS, "只有前 3 名精评");
    }


    #[test]
    fn diffeq_mode_emits_y_with_weight_4() {
        let mut next = stream(0xD1FFE9_u64);
        const N: u32 = 85_000;
        let mut x = 0u32;
        let mut y = 0u32;
        let mut dy = 0u32;
        for _ in 0..N {
            match draw_tag(&mut next, VarMode::DiffEq) {
                Function::VarX => x += 1,
                Function::VarY => y += 1,
                Function::VarDy => dy += 1,
                _ => {}
            }
        }
        assert_eq!(dy, 0, "y' 在两种模式里都不该出现");
        let ex = (N * 6) / 85;
        let ey = (N * 4) / 85;
        assert!((x as i64 - ex as i64).abs() < 400, "x 应约 {ex}，实测 {x}");
        assert!((y as i64 - ey as i64).abs() < 400, "y 应约 {ey}，实测 {y}");
    }

    #[test]
    fn normal_mode_never_emits_y() {
        let mut next = stream(0xA11CE);
        for _ in 0..20_000 {
            assert_ne!(draw_tag(&mut next, VarMode::Normal), Function::VarY);
        }
    }

    #[test]
    fn repair_fills_with_the_documented_mix() {
        // 空向量 ⇒ need=1，必须补 1 个 token；统计补齐 token 的类型比例
        let mut next = stream(0x9EE0_u64);
        let (mut c, mut x, mut y) = (0u32, 0u32, 0u32);
        const N: u32 = 40_000;
        for _ in 0..N {
            let mut v: Vec<Token> = Vec::new();
            repair(&mut v, VarMode::DiffEq, &mut next);
            match v.first().map(|t| t.tag) {
                Some(Function::Const) => c += 1,
                Some(Function::VarX) => x += 1,
                Some(Function::VarY) => y += 1,
                _ => {}
            }
        }
        // 反汇编：rand%50 <25 → 常数(50%)、<42 → x(34%)、否则 y(16%)
        assert!(((c as i64) - (N as i64) / 2).abs() < 500, "常数应约 50%，实测 {c}/{N}");
        assert!(((x as i64) - (N as i64 * 34 / 100) as i64).abs() < 500, "x 应约 34%，实测 {x}/{N}");
        assert!(((y as i64) - (N as i64 * 16 / 100) as i64).abs() < 500, "y 应约 16%，实测 {y}/{N}");
    }

    #[test]
    fn time_budget_stops_early_without_changing_semantics() {
        // 预算 0 秒 ⇒ 一代都不跑，种群原样返回（与游戏"共用同一出口"一致）
        let mut next = stream(0xB0D6_u64);
        let p = Params { generations: 50, population: 10, per_slot: 5, limit: 200 };
        let pop = initial_population(&p, VarMode::Normal, &mut next);
        let before = pop.clone();
        let out = evolve_with_budget(
            pop,
            &p,
            VarMode::Normal,
            &mut next,
            scalar_scorer(|_: &[Token]| 0.0),
            Some(0.0),
        );
        assert_eq!(out.len(), before.len());
    }

    #[test]
    fn game_budget_constant_matches_disassembly() {
        // 反汇编 0x51b7cf：cmp $0x19 ⇒ 25 秒
        assert_eq!(GAME_TIME_BUDGET_SECS, 25.0);
    }

    #[test]
    fn rng_u32_consumes_one_word_per_draw() {
        // 每 token 一次 u32 抽样（反汇编 0x4ef193）。这里验证：
        // 连续 16 次 next_u32 恰好用掉一个 ChaCha 输出块，第 17 次触发换块。
        use crate::_scaffold::rng::ChaCha12;
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&0x1234_5678_9ABC_DEF0u64.to_le_bytes());
        let mut r = ChaCha12::from_seed(&key);
        let mut got = Vec::new();
        for _ in 0..17 {
            got.push(r.next_u32());
        }
        assert_eq!(got.len(), 17);
        // 同种子两次运行必须一致
        let mut r2 = ChaCha12::from_seed(&key);
        let got2: Vec<u32> = (0..17).map(|_| r2.next_u32()).collect();
        assert_eq!(got, got2);
    }

    #[test]
    fn cross_from_yields_a_well_formed_function() {
        // 工厂只有**一个**父代（docs/99 §2）：取一棵完整子树再补齐，必须仍然合法
        let mut next = stream(0xC2055_u64);
        let parent = crate::math_function::parse("(((-0.35+x)/1.01)/-9.23)");
        for _ in 0..200 {
            let child = cross_from(&parent, VarMode::Normal, &mut next);
            assert!(is_well_formed(&child), "cross_from 产出不合法: {child:?}");
        }
    }

    #[test]
    fn factory_children_share_the_slot_parent() {
        // 每槽 1 克隆 + (per_slot-1) 个工厂子代，全部来自同一父代 sorted[i]
        let p = Params { generations: 1, population: 10, per_slot: 5, limit: 200 };
        let mut next = stream(0x5A1E_u64);
        let pop: Vec<Individual> = (0..10)
            .map(|i| {
                let toks = crate::math_function::parse(&format!("(* {} x)", i as f64 + 0.5));
                Individual::new(toks)
            })
            .collect();
        let out = next_generation(&pop, &p, VarMode::Normal, &mut next, |t, _| {
            Evaluation { score: 0.0, dist: DIST_SENTINEL, hits: 0 }
        });
        // 截断到 s = 10/5 = 2
        assert_eq!(out.len(), 2);
    }

    // -----------------------------------------------------------------------
    // 按士兵的进化层（docs/98 §3）
    // -----------------------------------------------------------------------

    /// 空场 + 无目标时，分数只由 `V²/100` 决定 ⇒ **token 最少的个体一定赢**
    /// （同分时 `sort_by` 是稳定排序，克隆排在子代前面，所以克隆保住第 0 名）。
    /// 于是"塞一条 1 token 的函数当种子"就能让存活者**确定**，测试不依赖 RNG。
    fn empty_field_targets() -> Vec<crate::graphwar_shot_calculator::settle::Target> {
        Vec::new()
    }

    /// 只有一条 `Const 0.0`（1 个 token）——空场下不可能被更短的打败。
    fn const_zero_pop() -> Vec<Individual> {
        vec![Individual::new(vec![Token::constant(0.0)])]
    }

    /// 同上但常数不同：用来证明"两个槽各存各的"。
    fn const_seven_pop() -> Vec<Individual> {
        vec![Individual::new(vec![Token::constant(7.0)])]
    }

    /// 造一个针对"空场"的进化器（难度参数调小，跑得快）。
    ///
    /// `flip` 显式给：真链路里它来自**队伍**（`docs/105`），这里只为把 `ScoringContext`
    /// 填满，与本测试的断言无关。
    fn empty_field_evolver<'a>(
        mask: &'a [u8],
        targets: &'a [crate::graphwar_shot_calculator::settle::Target],
    ) -> SoldierEvolver<'a> {
        let ctx = ScoringContext {
            x0: 10.0,
            y0: 0.0,
            flip: true,
            step: crate::SERVER_STEP,
            mask,
            targets,
            limit: 200,
        };
        SoldierEvolver::new(
            Params { generations: 1, population: 5, per_slot: 5, limit: 200 },
            VarMode::Normal,
            ctx,
        )
    }

    /// 开局：每个士兵一条**空**槽（`docs/98` §3.1 的 tick=15）。
    #[test]
    fn slots_start_empty_for_every_soldier() {
        let mut slots = SoldierFunctions::new();
        slots.register(2507);
        slots.register(2508);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots.tokens(2507), Some(&[][..]), "开局是空函数");
        assert_eq!(slots.updated_turn(2507), None, "还没被算过");
        assert!(slots.slot(2507).unwrap().is_empty());
        assert_eq!(slots.slot(2507).unwrap().owner_id, 2507, "owner_id = soldier_id");
        assert_eq!(slots.soldier_ids().collect::<Vec<_>>(), vec![2507, 2508]);
        assert!(slots.slot(9999).is_none(), "没登记过的士兵没有槽");
    }

    /// **不共享**：为士兵 A 进化，同一玩家的士兵 B 的槽一点不动（`docs/98` §3.3）。
    #[test]
    fn two_soldiers_of_one_player_have_separate_slots() {
        let mask = crate::plane::empty_mask();
        let targets = empty_field_targets();
        let ev = empty_field_evolver(&mask, &targets);
        let mut slots = SoldierFunctions::new();
        slots.register(2510);
        slots.register(2511);
        let mut next = stream(0xA11CE_u64);

        let a = ev.evolve_soldier(&mut slots, 2510, 1, const_zero_pop(), &mut next);
        // A 写进去了（空场下 1 token 的种子必赢，内容确定 = [Const 0.0]）
        assert_eq!(a.soldier_id, 2510);
        assert_eq!(a.tokens, vec![Token::constant(0.0)]);
        assert_eq!(slots.tokens(2510), Some(a.tokens.as_slice()));
        assert_eq!(slots.updated_turn(2510), Some(1));
        // B 一点没动 —— 这就是"不共享"
        assert_eq!(slots.tokens(2511), Some(&[][..]), "B 的槽仍为空");
        assert_eq!(slots.updated_turn(2511), None);

        let b = ev.evolve_soldier(&mut slots, 2511, 1, const_seven_pop(), &mut next);
        assert_eq!(b.soldier_id, 2511);
        assert_eq!(b.tokens, vec![Token::constant(7.0)]);
        // 两个槽**各存各的**：同一个玩家、同一条链路，内容互不相同
        assert_eq!(slots.tokens(2510), Some(a.tokens.as_slice()));
        assert_eq!(slots.tokens(2511), Some(b.tokens.as_slice()));
        assert_ne!(slots.tokens(2510), slots.tokens(2511), "两个士兵的槽内容不同");
        // 再写回一次 A 也不会碰到 B
        ev.evolve_soldier(&mut slots, 2510, 2, const_seven_pop(), &mut next);
        assert_eq!(slots.updated_turn(2511), Some(1), "B 的回合号没被 A 顶掉");
        assert_eq!(slots.updated_turn(2510), Some(2));
        assert_eq!(slots.tokens(2511), Some(b.tokens.as_slice()), "B 的函数也没被 A 顶掉");
    }

    /// **每回合重算**：同一士兵下一回合再调用会**覆盖**上一次结果（`docs/98` §3.2：
    /// tick 168 / 12269 / 26038 三次文本都不一样）。
    #[test]
    fn same_soldier_is_recomputed_next_turn() {
        let mask = crate::plane::empty_mask();
        let targets = empty_field_targets();
        let ev = empty_field_evolver(&mask, &targets);
        let mut slots = SoldierFunctions::new();
        slots.register(2507);
        let mut next = stream(0xBEEF_u64);

        // 第 1 回合：种子是 [Const 0.0]
        let t1 = ev.evolve_soldier(&mut slots, 2507, 1, const_zero_pop(), &mut next);
        assert_eq!(t1.tokens, vec![Token::constant(0.0)]);
        assert_eq!(slots.updated_turn(2507), Some(1));

        // 第 2 回合：种子换成 [Const 7.0] ⇒ 槽被**覆盖**，不是"沿用上次的结果"
        let t2 = ev.evolve_soldier(&mut slots, 2507, 2, const_seven_pop(), &mut next);
        assert_eq!(slots.updated_turn(2507), Some(2));
        assert_eq!(slots.tokens(2507), Some(t2.tokens.as_slice()));
        assert_eq!(t2.tokens, vec![Token::constant(7.0)]);
        assert_ne!(t1.tokens, t2.tokens, "同一士兵两回合的函数不同 ⇒ 每回合重算");
        // 上一回合的结果**没有**被缓存下来复用：槽里只有这一次的
        assert_eq!(slots.tokens(2507), Some(&[Token::constant(7.0)][..]));
    }

    /// 进化结果就是"存活者第 0 个"（`evolve` 返回按分数降序的存活者）。
    #[test]
    fn slot_holds_the_best_survivor() {
        let mask = crate::plane::empty_mask();
        let targets = empty_field_targets();
        let ev = empty_field_evolver(&mask, &targets);
        let mut slots = SoldierFunctions::new();
        let mut next = stream(0x5157_u64);
        let out = ev.evolve_soldier(&mut slots, 42, 7, const_zero_pop(), &mut next);
        assert_eq!(out.survivors.len(), 1, "种群 5 / 每槽 5 ⇒ s = 1");
        assert_eq!(out.tokens, out.survivors[0].tokens, "槽里是第 0 名");
        assert_eq!(slots.tokens(42), Some(out.tokens.as_slice()));
        assert_eq!(out.turn, 7);
    }

    /// 预算接的是真游戏那档（25 s，`docs/93`）。
    #[test]
    fn budget_is_wired_to_the_game_value() {
        let mask = crate::plane::empty_mask();
        let targets = empty_field_targets();
        let ev = empty_field_evolver(&mask, &targets);
        assert_eq!(ev.budget_secs, None, "默认不限时（测试可复现）");
        assert_eq!(ev.with_budget(Some(GAME_TIME_BUDGET_SECS)).budget_secs, Some(25.0));
    }

    /// `soldier_id` 由**外部**给出，本层不选射手：传什么 id 就写谁的槽。
    #[test]
    fn soldier_id_comes_from_the_caller_not_the_ai() {
        let mask = crate::plane::empty_mask();
        let targets = empty_field_targets();
        let ev = empty_field_evolver(&mask, &targets);
        let mut slots = SoldierFunctions::new();
        let mut next = stream(0xC0FFEE_u64);
        // 同一个进化器先后为三个不同士兵算，槽各自独立
        for id in [7444, 7445, 7446] {
            let out = ev.evolve_soldier(&mut slots, id, 1, const_zero_pop(), &mut next);
            assert_eq!(out.soldier_id, id);
            assert_eq!(slots.slot(id).unwrap().owner_id, id);
        }
        assert_eq!(slots.len(), 3);
        assert_eq!(slots.soldier_ids().collect::<Vec<_>>(), vec![7444, 7445, 7446]);
    }
}
