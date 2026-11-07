//! 原版 `src/polish_notation_converter.rs` —— **中缀 → 波兰（前缀）那一趟**。
//!
//! ## 这个模块不是死代码（2026-11-07 结案，`docs/16-client.md` §30.1）
//!
//! 唯一带符号的函数是 `PolishNotationConverter::reorder_recursive`
//! （x86-64 目标 `0x4f5450`，arm64 2.0.60 `0x44db78`，324 字节）。它有**两个**调用点，
//! 都在 `MathFunction::new`（x86 `0x4f5450` ← `0x4edd1f`；arm64 `0x4471b4`）：
//!
//! ```text
//! x86 0x4edcc9  movaps 0x110(%rsp),%xmm0     ; 输入 Vec<FunctionToken> {cap,ptr,len}
//!      0x4edce9  movq $0x0, 0xa8(%rsp)       ; 输出 Vec::new()（cap=0）
//!      0x4edcf5  movq $0x8, 0xb0(%rsp)       ;              dangling ptr = 8（16 字节对齐）
//!      0x4edd01  movq $0x0, 0xb8(%rsp)       ;              len=0
//!      0x4edd0d  test %rdx,%rdx ; je 跳过    ; —— len == 0 就不 call（Rd 空）
//!      0x4edd12  dec %rdx                    ; end = len-1
//!      0x4edd15  lea 0x90(%rsp),%rdi         ; &converter
//!      0x4edd1d  xor %esi,%esi               ; start = 0
//!      0x4edd1f  call 0x4f5450
//! ```
//!
//! ⇒ **"`MathFunction::new` 自己就做分词"这句话只对了一半**（`docs/03-function-language.md` §13）：
//! 它确实自己**分词**（正则 + `FunctionToken::new`），但**括号与优先级完全不在它那里**——
//! 分词产物是**中缀**的（含 tag 18 `(` / 19 `)` 两个 token），
//! 变成前缀靠的就是本模块。⇒ 不是死代码，是解析器的**后半段**，删掉它 `sin(x)^2` 的语义就没了。
//!
//! ## 结构体布局（逐指令读出，arm64/x86 一致）
//!
//! 原版字段顺序被 rustc 重排过（`repr(Rust)`），内存里是 `Vec` 的 `(cap, ptr, len)`：
//!
//! | 偏移 | 字段 |
//! |---|---|
//! | `+0x00` | `input : Vec<FunctionToken>` 的 **cap** |
//! | `+0x08` | `input : Vec<FunctionToken>` 的 **ptr**（`0x8(%rdi)`，扫描主指针） |
//! | `+0x10` | `input : Vec<FunctionToken>` 的 **len**（越界 panic 用，锚 `36:52`） |
//! | `+0x18` | `output: Vec<FunctionToken>` 的 **cap**（`0x18(%rdi)`，push 前比较） |
//! | `+0x20` | `output: Vec<FunctionToken>` 的 **ptr**（`0x20(%rdi)`） |
//! | `+0x28` | `output: Vec<FunctionToken>` 的 **len**（`0x28(%rdi)`，每 push 一次 +1） |
//!
//! 本 crate 直接用两个 `Vec` 表达，不模拟字段重排（重排是**布局**细节，
//! 语义还原不需要；要逐字节对拍布局时再看上表）。
//!
//! ## 算法：**递归地找"这一层的根"**
//!
//! 不是 shunting-yard（那需要一个操作符栈）。原版是"**递归切分**"：
//!
//! 1. 在 `[start, end]`（**闭区间**，`end = len-1`）里扫一遍，用 `depth` 数括号
//!    （tag 18 `+1`、19 `-1`，见 [`crate::Function::LeftBracket`] / [`crate::Function::RightBracket`]）；
//! 2. 只在**最浅一层**（`depth` 最小）的 token 里挑**优先级数值最大**的那个——
//!    数值大 = 结合最松 = 就是这一层表达式的**根**（见 [`PRECEDENCE`]）；
//! 3. 把根 **push 到 output**（这就是前缀序），然后：
//!    * 根是一元（`neg` = 5，或 `sqrt..tan` = 9..=16）⇒ 继续扫 `[root+1, end]`（**不递归**）；
//!    * 根是原子（`0/1/2/17`，及不可能当根的括号 `18/19`）⇒ 本层结束，返回；
//!    * 根是二元（`+ - * / ^`）⇒ 先递归 `[start, root-1]`，再递归 `[root+1, end]`；
//!      **`+`/`-` 缺哪边就补一个 `CONST 0`**（见 `docs/03-function-language.md` 的
//!      "`+` fills missing operands with CONST 0"——补洞的就是这里，不是分词器）。
//!
//! ## 三个"看起来像 bug、其实是原版行为"的点
//!
//! 1. **平级一律左结合**。同优先级的两个二元操作符相遇时，判据是
//!    `PRECEDENCE[new] >= PRECEDENCE[best]`（`0x4f55ca` `jae`），`>=` 而不是 `>` ⇒
//!    后出现的当根 ⇒ `a-b-c` = `(- (- a b) c)`、`a^b^c` = `(^ (^ a b) c)`。
//!    `docs/03-function-language.md` 里"`^` 是左结合"的**机制**就在这里。
//! 2. **一元函数是右结合**（`sin cos x` ⇒ `sin (cos x)`）：平级时走的是
//!    `PRECEDENCE[best] < PRECEDENCE[new]` 这条**严格**小于的判据，先出现的当根。
//! 3. **词法器不产 `neg`（tag 5）**：`FunctionToken::new("-")` 给的是**二元减**（tag 4）。
//!    所以 `-x` 走的是"`-` 当根 + 缺左操作数补 `CONST 0`"⇒ 前缀序是 `(- 0 x)`。
//!    转换器本身**认识** tag 5（走一元分支），但它的**产生点本轮未定位**（`未知`；
//!    候选：`FunctionRandomizer::mutate_fine_tune` 或 `MathFunction::new` 里的一趟后处理）。
//! 4. **`depth` 用有符号比较**（`jge`，`0x4f5536`）。正常输入里 `depth` 不会为负，
//!    但多一个 `)` 会让它跌到 `-1`，而 `-1` 比任何合法深度都"更浅"⇒ 会被优先当根。
//!    这条**没在真实战报里观测到**，标 `推断`：不保证原版作者有意为之。

use crate::{Function, Token};

/// 运算符优先级，**下标 = tag**（`0..=17`）。
///
/// 逐项取自 `.rodata`：x86 目标 `0x1c4838`（`0x1c4888` 是同一张表的第二份拷贝，
/// 链接器没合并）；arm64 2.0.60 里同一个函数用的也是同一套数值。
///
/// ```text
/// tag  0  1  2   3  4   5   6  7   8   9 10 11 12 13 14 15 16  17
/// 值   2  2  2   7  7   4   6  6   4   3  3  3  3  3  3  3  3   2
///      x  y  y'  +  -  neg  *  /   ^  sqrt log ln exp abs sin cos tan const
/// ```
///
/// **数值越大 = 结合越松**（越靠近树根）。所以 `+`/`-` = 7 最松，
/// 原子 = 2 最紧。判据全部按"取最大"写，容易和直觉反过来，读代码时留神。
///
/// 下标 18/19（括号）原版会**越界读到**下一张表的头（`0x1c487c`/`0x1c4880` = 2/1），
/// 但括号永远不会被选成根（扫描时只 `+1/-1` 深度），所以这条路径**不可达**；
/// 这里显式给 0 并标注，不去复刻越界。
pub const PRECEDENCE: [u32; 20] = [
    2, 2, 2, // VarX, VarY, VarDy
    7, 7, // Add, Sub
    4, // UnaryMinus
    6, 6, // Mul, Divide
    4, // Pow
    3, 3, 3, 3, // Sqrt, Log, Ln, Exp
    3, 3, 3, // Abs, Sin, Cos
    3, // Tan
    2, // Const
    0, 0, // LeftBracket, RightBracket —— 原版不可达，见上
];

/// `bt` 的掩码 1：**不是二元操作符**的 tag 集合。
///
/// 逐值取自 x86 `0x4f554f` `mov $0x3fe27,%ebx`：`{0,1,2,5,9,10,11,12,13,14,15,16,17}`。
/// 换句话说：原子（0/1/2）、一元负号（5）、一元函数（9..=16）、常量（17）。
/// **`+ - * / ^`（3/4/6/7/8）不在里面**，括号（18/19）也不在。
pub const MASK_NOT_BINARY: u64 = 0x3fe27;

/// `bt` 的掩码 2：掩码 1 **加上括号**。
///
/// 取自 x86 `0x4f555a` `mov $0xffe27,%ebx` = `{0,1,2,5,9..=17,18,19}`。
/// 用途是判"当前最优是不是二元操作符"——是，就走左结合的 `>=` 分支。
pub const MASK_NOT_BINARY_OR_BRACKET: u64 = 0xffe27;

/// "原子/常量，选到它就收工"掩码。
///
/// 取自 x86 `0x4f5688` `mov $0xe0007,%ecx` = `{0,1,2,17,18,19}`。
pub const MASK_LEAF: u64 = 0xe0007;

/// 中缀 → 前缀的转换器。**与原版同名同结构**（两个 `Vec`）。
///
/// 原版没有 `new` 符号：结构体是在 `MathFunction::new` 里**就地构造**的
/// （`0x4eded…` 三句 `movq` 写 `output` 的 cap/ptr/len），所以本模块
/// `FunctionToken` 数量是 1（`reorder_recursive`），不是 2。
#[derive(Debug, Default, Clone)]
pub struct PolishNotationConverter {
    /// 正则切出来的**中缀** token 序列（含 tag 18/19）。
    pub input: Vec<Token>,
    /// 转换产物：**前缀**（波兰）序列，**不含**括号 token。
    pub output: Vec<Token>,
}

impl PolishNotationConverter {
    /// 拿一段中缀 token 建转换器。
    pub fn new(input: Vec<Token>) -> Self {
        Self { input, output: Vec::new() }
    }

    /// 一趟转完，返回前缀序列。
    ///
    /// 对应 x86 `0x4edd0d..0x4edd1f`：`len == 0` 时**不调用** `reorder_recursive`
    /// （空输入 ⇒ 空输出）。
    pub fn convert(mut self) -> Vec<Token> {
        if !self.input.is_empty() {
            let last = self.input.len() - 1;
            self.reorder_recursive(0, last);
        }
        self.output
    }

    /// 便捷入口：中缀 token → 前缀 token。
    pub fn reorder(input: Vec<Token>) -> Vec<Token> {
        Self::new(input).convert()
    }

    /// 原版 `PolishNotationConverter::reorder_recursive`（x86 `0x4f5450`，arm64 `0x44db78`）。
    ///
    /// `start` / `end` 都是**下标**且 `end` 是**闭区间上界**（`0x4edd12` 的 `dec %rdx`
    /// 把 `len` 变成 `len-1` 就是这个意思）。`start > end` 直接返回
    /// （`0x4f545e` `cmp %rdx,%rsi ; jbe` 之后走 `0x4f5463` 的出口）。
    fn reorder_recursive(&mut self, mut start: usize, end: usize) {
        if start > end {
            return;
        }
        loop {
            // 0x4f54ca: 只跟 **len** 比（不是 end）
            if start >= self.input.len() {
                return;
            }

            // ---- 扫描 [start, end)：挑"最浅一层里结合最松"的那个当根 ----
            let mut best: Option<usize> = None;
            let mut best_depth: i32 = i32::MAX; // 0x4f54d7 mov $0x7fffffff
            let mut depth: i32 = 0; // xor %eax,%eax
            let mut i = start;
            // **闭区间**：机器码的出口判据在"处理完 token 之后"（0x4f5590
            // `cmp %r15,%rdi ; jae`），所以 `i == end` 那个 token 也会被扫到。
            while i <= end {
                // 0x4f5590: cmp %r15,%rdi ; jae 结束扫描
                let tag = self.input[i].tag;
                match tag {
                    // 0x4f550c/0x4f5510: cmp $0x12 ; je → inc %eax
                    Function::LeftBracket => depth += 1,
                    // 0x4f5512/0x4f5518: cmp $0x13 ; → dec %eax
                    Function::RightBracket => depth -= 1,
                    _ => {
                        // 0x4f5533: cmp %r9d,%eax ; jge → 只看 == 的分支
                        if depth < best_depth {
                            best = Some(i);
                            best_depth = depth;
                        } else if depth == best_depth {
                            if let Some(b) = best {
                                if take_new(self.input[b].tag, tag) {
                                    best = Some(i);
                                }
                            }
                        }
                    }
                }
                i += 1;
            }

            // ---- 0x4f5610: 没挑到东西（比如整段只有括号）就收工 ----
            let Some(root) = best else { return };

            // ---- 0x4f5490: push root ----
            let root_tag = self.input[root].tag;
            self.output.push(self.input[root]);

            // ---- 0x4f54b4: tag-9 < 8 ⇒ 一元函数，接着扫右边（不递归） ----
            if is_unary_function(root_tag) || root_tag == Function::UnaryMinus {
                // 0x4f54c0: r13 = r12+1 ; cmp %r15,%r12 ; jae 出口
                if root >= end {
                    return;
                }
                start = root + 1;
                continue;
            }

            // ---- 0x4f5688: 原子/常量（含理论上的括号）⇒ 本层结束 ----
            if MASK_LEAF & (1u64 << (root_tag as u64)) != 0 {
                return;
            }

            // ---- 二元：左边、右边各递归一次；`+`/`-` 补 CONST 0 ----
            let fills_with_zero = matches!(root_tag, Function::Add | Function::Sub);
            let len_before_left = self.output.len();
            if root > 0 {
                self.reorder_recursive(start, root - 1); // 0x4f56b3
            }
            if fills_with_zero && self.output.len() == len_before_left {
                // 0x4f56f7: movq $0x11,(%rax,%rbx,1) —— tag 17 = CONST，值 0.0
                self.output.push(Token::constant(0.0));
            }
            let len_before_right = self.output.len();
            self.reorder_recursive(root + 1, end); // 0x4f5721
            if fills_with_zero && self.output.len() == len_before_right {
                // 0x4f575f: 同上，右边也补
                self.output.push(Token::constant(0.0));
            }
            return;
        }
    }
}

/// `sqrt log ln exp abs sin cos tan`（tag `9..=16`）—— 机器码里是 `lea -0x9(%rax),%ecx ;
/// cmp $0x8,%ecx ; jae` 即 `tag-9 < 8`（`0x4f54b4`）。
fn is_unary_function(tag: Function) -> bool {
    (Function::Sqrt as u64..=Function::Tan as u64).contains(&(tag as u64))
}

/// 平级时"要不要用新 token 顶掉当前最优"——x86 `0x4f5547..0x4f55d0` 的全部判据。
///
/// 三条互斥路径，照机器码的分支顺序写：
///
/// 1. **新的是原子/一元/常量**（掩码 [`MASK_NOT_BINARY`]）⇒ `PRECEDENCE[best] < PRECEDENCE[new]`
///    才换（`0x4f557b` 的 `jb`）。一元函数平级时 `3 < 3` 不成立 ⇒ **保留先出现的** ⇒ 右结合。
/// 2. **新的和当前最优都是二元** ⇒ 按新 tag 分三档比较
///    （`0x4f55b8` `+`/`-` 用 7、`0x4f55d2` `*`//`/` 用 6、`0x4f55ec` `^` 用 4），
///    判据是 `档位 >= PRECEDENCE[best]` 的 **`>=`**（`jae`）⇒ 平级换新的 ⇒ **左结合**。
/// 3. 其余（新的是二元、最优是一元/原子）⇒ 回到路径 1 的严格小于
///    （`0x4f555f` `bt` 命中 ⇒ 不跳 `0x4f55a3`，直接落到 `0x4f5565`）。
fn take_new(best: Function, new: Function) -> bool {
    let prec = |t: Function| -> u32 { PRECEDENCE[t as u64 as usize] };
    if MASK_NOT_BINARY & (1u64 << (new as u64)) != 0 {
        // 路径 1
        return prec(best) < prec(new);
    }
    if MASK_NOT_BINARY_OR_BRACKET & (1u64 << (best as u64)) == 0 {
        // 路径 2：两个都是二元操作符
        let new_rank = match new {
            Function::Add | Function::Sub => 7,
            Function::Mul | Function::Divide => 6,
            Function::Pow => 4,
            // 机器码里这条分支的跳转表只覆盖 tag 3..=8（`(new_tag-3)` 索引 0..=5）；
            // 走不到这里。
            _ => return prec(best) < prec(new),
        };
        return new_rank >= prec(best);
    }
    // 路径 3
    prec(best) < prec(new)
}
