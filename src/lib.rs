//! Graphwar II —— 从 `libgraphwarrust.so` 静态逆向复原的实现。
//!
//! 本 crate 只放**已被二进制证据支持**的东西。凡未验证的模块一律留空并标注 TODO，
//! 不用"猜一个能跑的实现"来充数。
//!
//! 证据与验证方法见 `docs/06-verification.md`；算法细节见 `docs/`。
//!
//! 目标二进制：`libgraphwarrust.so`（GDExtension, Rust, stripped, x86-64）。
//! 地址约定：Ghidra/REA 地址 = 文件地址 + `0x100000`。

#![allow(dead_code)]

/// `enum Function` 的判别值，等价于原程序里 16 字节 token 的 `tag` 字段。
///
/// 编号不是猜的：`.rodata` 里这些变体名以连续字符串形式存在
/// （`VAR_X VAR_Y VAR_DY ADD ... CONST LEFT_BRACKET RIGHT_BRACKET`），
/// 与解析器里产生的编号一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum Function {
    VarX = 0,
    VarY = 1,
    VarDy = 2,
    Add = 3,
    Sub = 4,
    UnaryMinus = 5,
    Mul = 6,
    Divide = 7,
    Pow = 8,
    Sqrt = 9,
    /// 注意：是 log10，不是自然对数。
    Log = 10,
    Ln = 11,
    Exp = 12,
    Abs = 13,
    Sin = 14,
    Cos = 15,
    Tan = 16,
    Const = 17,
    LeftBracket = 18,
    RightBracket = 19,
}

/// 原程序里的 token：`{ tag: u64, value: f64 }`，16 字节。
///
/// `value` 只在 [`Function::Const`] 时有意义。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token {
    pub tag: Function,
    pub value: f64,
}

impl Token {
    pub const fn op(tag: Function) -> Self {
        Self { tag, value: 0.0 }
    }

    pub const fn constant(value: f64) -> Self {
        Self { tag: Function::Const, value }
    }
}

// ---------------------------------------------------------------------------
// 调参常量：全部直接读自 `.rodata`，地址见图注。改动前先读 docs/04-path-sampling.md。
// ---------------------------------------------------------------------------

/// 棋盘 X 半宽，`0x19fbc8`。点入列前硬检查 `|x| <= 25`。
pub const MAX_ABS_X: f64 = 25.0;

/// 棋盘 Y 半高，`0x19fd40`。
pub const MAX_ABS_Y: f64 = 15.0;

/// 弦长收敛容差，`0x19fc78`。判据是 `|L - step| < 0.001`。
pub const LENGTH_TOL: f64 = 0.001;

/// 点数预算基数，`0x19f8c0`：`n = (200.0 / step) as i32`（Rust 饱和转换）。
pub const TOTAL_LEN: f64 = 200.0;

/// 每个点最多二分多少次。
pub const MAX_BISECT_STEPS: u32 = 100;

/// 服务端固定使用的步长，`0x3f847ae147ae147b`（= 0.01）。
pub const STEP_DEFAULT: f64 = 0.01;

/// π 的别名 `n` 的值。
pub const PI: f64 = std::f64::consts::PI;

/// 常量记号 `e`。
pub const E: f64 = std::f64::consts::E;

// ---------------------------------------------------------------------------
// 模块状态
// ---------------------------------------------------------------------------
//
// ✅ 已验证（逐位对拍）
//    - function::parse   文本 -> 前缀 token       @0x4ed670  docs/03-function-language.md
//    - function::eval    前缀 token -> f64        @0x4eb9d0  docs/03-function-language.md
//    - function::fold    常量子树折叠             @0x4eb830  docs/03-function-language.md
//    - path::calculate   点序列采样               @0x3c2530  docs/04-path-sampling.md
//
// 🔶 调用链已还原、内部细节待补
//    - shot::correct     修正常数 c = y0 - f(±x0) @0x3c5f70  docs/05-shot-pipeline.md
//
// ⬜ 待逆向（见 docs/02-module-map.md）
//    - plane（坐标语义）、turn（回合）、life（生命）、level（关卡）、ai、net

pub mod function;
pub mod path;
