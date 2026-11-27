extends RefCounted
class_name GwExpr
## 函数表达式求值器 —— **规则在 Rust**，本文件只是薄壳（`client/src/core/expr.gd` 改前 229 行）。
##
## 为什么改：T35 审计（`docs/06` §90）把这段判成 **C 档** —— 同一件事在 `src/math_function.rs`
## 与这里各有一份实现。原版的解析器（`@0x4ed670`）与求值器（`@0x4eb9d0`）早已**逐位**复原在
## Rust 侧（`docs/03-function-language.md`、`docs/06` §30），而这里那份是照 `docs/03` 手抄的
## 第二实现。两份真相源的代价**已经出现**：手抄版在四处与真值不同（逐条见
## `fixtures/controller_migration2_ledger.tsv` 的 E04/E07/E08/E09 行）：
##
## | # | 改前这里的做法 | 真值（Rust / 二进制） |
## |---|---|---|
## | 1 | 未匹配字符 ⇒ 报错"不认识的字符" | 词法器**跳过**未匹配字符（`Π → []`） |
## | 2 | 栈上 token 不足 ⇒ 返回 `NaN` | 返回 **`0.0`**（"缺操作数也照样出图"的来源） |
## | 3 | `sqrt(-1)` ⇒ 正 `NaN` | `sqrtsd` 给**负** NaN（`fff8…`） |
## | 4 | 除零一律正 `NaN` | `1/0 = +inf`、`1/-0 = -inf`、`0/0 = 负 NaN`（`divsd`） |
##
## 现在 `_compile` 只是 `GwMathExpr.parse()`、`eval()` 只是 `GwMathExpr.eval_at()`：
## 词法（含 ASCII 折叠与 `pi`/`π`→`n`）、隐式乘法、优先级、中缀→前缀、求值、8 个数学函数
## 全在 Rust。本文件**没有**任何解析或算术逻辑 —— 再改规则请改 `src/math_function.rs`。
##
## 与内核的差别（保留这条是因为它仍然是事实，只是不再是"两套实现"）：
##   * 内核按**弦长 0.01** 二分采样（`src/graphwar_path_calculator.rs`）；`core_bridge.gd`
##     的界面预览按 x 等步长折线近似 —— **采样**差异在预览层，不在本文件；
##   * 本文件只做"在这条表达式上求一个值"，采样与画由调用方负责。

## 已支持的内置函数（**只为文档/自检保留**；判定在 Rust 的 `math_function::lex`）。
const FUNCS := {
	"sin": 1, "cos": 1, "tan": 1, "sqrt": 1, "abs": 1,
	"ln": 1, "log": 1, "exp": 1,
}

var _m: GwMathExpr = null
var _error := ""

func get_error() -> String:
	return _error

func is_ok() -> bool:
	return _m != null and _error == ""

# ---------------------------------------------------------------- 编译

static func compile(expr_text: String) -> GwExpr:
	var self_ := GwExpr.new()
	self_._compile(expr_text)
	return self_

## 解析。**规则全在 Rust**（`math_function::parse`，`0x4ed670`）。
func _compile(expr_text: String) -> void:
	_error = ""
	_m = GwMathExpr.new()
	if not _m.parse(expr_text):
		_error = _m.error()

# ---------------------------------------------------------------- 求值

## 在 x = xv 处求值（`y = f(x)` 口径：y=0、dy=0）。
##
## 真求值器**没有**错误路径：token 序列不够时返回 `0.0`（不是 NaN）。所以这里也不再
## 自己判"栈不合法"—— 要判"能不能画"请看 `is_ok()`。
func eval(xv: float) -> float:
	if not is_ok():
		return NAN
	return _m.eval_at(xv)
