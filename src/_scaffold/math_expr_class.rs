//! **不是原版的东西**：把 [`crate::math_function`]（原版 `graphwar_function_input` 的
//! **解析器** `@0x4ed670` 与**求值器** `@0x4eb9d0` 的复原实现）接成 GDExtension 类
//! `GwMathExpr`，让 `client/src/core/expr.gd` 只剩一个薄壳。
//!
//! # 为什么会有这一份（T58 / `docs/06` §90 的 C 档第 4 段）
//!
//! `client/src/core/expr.gd`（改前 229 行）是一台**完整的第二实现**：自己词法、自己补
//! 隐式乘法、自己算优先级（`_prec`）、自己 shunting-yard 转 RPN、自己栈式求值、自己实现
//! 8 个数学函数与 5 个运算符。而真值早就复原在 Rust 里：
//! `src/math_function.rs` 的 `lex`（`0x4ed670` 解析器）/ `parse` / `eval`（`0x4eb9d0` 求值器），
//! 逐位对拍记录见 `docs/03-function-language.md` 与 `docs/06` §30。
//!
//! 同一规则两份真相源的后果**已经发生了**：`.gd` 那台在四个地方与真值不同
//! （逐条证据见 `fixtures/controller_migration2_ledger.tsv` 的 E04/E07/E08/E09 行）：
//!
//! | # | `.gd` 改前的行为 | 真值（Rust / 二进制） | 证据 |
//! |---|---|---|---|
//! | 1 | 未匹配字符 ⇒ 报错「不认识的字符」 | 词法器**跳过**未匹配字符（`Π → []`） | 逐位，`docs/03` |
//! | 2 | token 序列不足 ⇒ 返回 `NaN` | **返回 `0.0`**（"缺操作数也照样出图"的来源） | 逐位，`docs/03` |
//! | 3 | `sqrt(-1)` ⇒ 正 `NaN` | `sqrtsd` 给**负** NaN `fff8…` | 逐位，`docs/06` §30 |
//! | 4 | 除零一律正 `NaN` | `1/0=+inf`、`0/0=负 NaN`（`divsd`） | 逐位，`docs/06` §30 |
//!
//! 所以本文件**不新增任何判据**：它只把 `math_function::{lex,parse,eval}` 包成一句
//! GDScript 能调的 API，`.gd` 侧不再有任何解析/求值逻辑。
//!
//! # 接口
//!
//! ```gdscript
//! var e := GwMathExpr.new()
//! var ok := e.parse("sin(x)+2")   # bool；词法器/解析器都在 Rust
//! e.error()                        # "" 或原因（只有"空表达式"一条，因为真词法器不报错）
//! e.eval_at(1.5)                   # float（`y = f(x)` 口径：y=0、dy=0）
//! e.token_count()                  # 前缀 token 个数（自检用）
//! e.prefix_tags()                  # PackedInt32Array，token 的 Function 判别值（自检用）
//! e.provenance()                   # 出处/档位（自检、文档复核用）
//! ```
//!
//! 为什么 `eval_at` 不返回"错误码"：真求值器**没有**错误路径 —— 它是递归下降读前缀序列，
//! 序列不够就返回 `0.0`。给 GDScript 造一个"成功/失败"二元判断就是把推断写成结论。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::math_function;

/// 表达式求值器（scaffold；原版没有这个类 —— 原版是 `GraphwarFunctionInput` 内部的
/// `Vec<Function>` + 自由函数 `eval`）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwMathExpr {
    base: Base<RefCounted>,

    /// 文本（只留作诊断/出处，不参与求值）。
    text: String,
    /// 解析出来的**前缀** token 序列（就是原版 `Vec<Function>` 的口径）。
    tokens: Vec<crate::Token>,
    /// 解析失败原因；空 = 成功。真词法器不报错 ⇒ 只会有"空表达式"这一条。
    error: String,
}

#[godot_api]
impl IRefCounted for GwMathExpr {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, text: String::new(), tokens: Vec::new(), error: String::new() }
    }
}

#[godot_api]
impl GwMathExpr {
    /// 解析一段函数文本。返回 `true` = 有可求值的 token 序列。
    ///
    /// 与真解析器同一入口：[`math_function::parse`]（`0x4ed670`）。**不**在 GDScript 侧
    /// 做任何预处理（不补 `*`、不小写化、不切词）。
    #[func]
    fn parse(&mut self, text: GString) -> bool {
        let s = text.to_string();
        self.text = s.clone();
        self.tokens = math_function::parse(&s);
        self.error = if self.tokens.is_empty() {
            // 真词法器遇到不认识的东西是**跳过**，所以"空 token 串"是唯一能确定的失败。
            if s.trim().is_empty() {
                "空表达式".to_string()
            } else {
                "无法解析（词法器没有产出任何 token）".to_string()
            }
        } else {
            String::new()
        };
        !self.tokens.is_empty()
    }

    /// 解析失败原因（空串 = 没有失败）。
    #[func]
    fn error(&self) -> GString {
        GString::from(self.error.as_str())
    }

    /// 在 `x = xv` 处求值（`y = f(x)` 口径：`y`/`dy` 传 0）。
    ///
    /// 走 [`math_function::eval`]（`0x4eb9d0`）。序列为空时真程序的语义是返回 `0.0`，
    /// 这里照抄 —— 调用方要判"能不能画"请先看 [`Self::parse`] 的返回值。
    #[func]
    fn eval_at(&self, xv: f64) -> f64 {
        math_function::eval(&self.tokens, xv, 0.0, 0.0)
    }

    /// 前缀 token 个数（自检/诊断用）。
    #[func]
    fn token_count(&self) -> i64 {
        self.tokens.len() as i64
    }

    /// 前缀 token 的 `Function` 判别值序列（自检用；判别值就是原版枚举的声明序）。
    #[func]
    fn prefix_tags(&self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        for t in &self.tokens {
            out.push(t.tag as i32);
        }
        out
    }

    /// 出处与档位（自检、文档复核用；客户端界面不读）。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("parser", &GString::from("math_function::parse"));
        d.set("parser_addr", &GString::from("0x4ed670"));
        d.set("evaluator", &GString::from("math_function::eval"));
        d.set("evaluator_addr", &GString::from("0x4eb9d0"));
        d.set(
            "evidence",
            &GString::from(
                "逐位：解析器/求值器与真二进制逐位对拍（docs/03、docs/06 §30）；\
                 sqrt=sqrtsd 0x4ebb26、div=divsd 0x4ebb4e、log10=GOT 0x9bc270、ln=GOT 0x9bbec0",
            ),
        );
        d.set("grade", &GString::from("逐位"));
        d
    }
}
