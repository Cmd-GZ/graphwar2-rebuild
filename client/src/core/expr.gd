extends RefCounted
class_name GwExpr
## 函数表达式求值器 —— **只给界面预览用，不是内核**。
##
## 为什么会有这个东西：真实采样器（等弦长二分、`step=0.01`、`(200/step) as i32` 点数上限、
## 边界 `|x|<=25 / |y|<=15`）在 Rust 内核 `src/graphwar_path_calculator.rs` 里，已逐位复原并验证
## （`docs/04`、`docs/89`）。内核目前只向 GDScript 暴露了 `GW2Rebuild.ping()`
## （`src/_scaffold/gdext_class.rs`），**没有** `path_calculate`。为了让界面在没加载内核时也能画条线，
## 这里实现一个**只求值、不采样**的小求值器；采样由调用方用固定 dx 折线近似。
##
## 与内核的差别（必须写清）：
##   * 内核按**弦长 0.01** 二分采样（曲线上的相邻点距离恒定）；这里是**按 x 等步长**取样，
##     所以陡峭段看起来点更稀、长度也不等于真实弹道；
##   * 内核的 `+` 缺操作数会补 `CONST 0`、`^` 左结合、隐式乘法、`log`=log10、
##     `pi`/`π`/`n`=π —— 这几条**是内核的怪癖**，本求值器照抄（因为它们是界面要展示的语言），
##     但 RPN 求值不会做内核的"栈式容错"，表达式非法时直接报错。

## 已支持的内置函数（名字 → 可调用）。
const FUNCS := {
	"sin": 1, "cos": 1, "tan": 1, "sqrt": 1, "abs": 1,
	"ln": 1, "log": 1, "exp": 1,
}

var _rpn: Array = []
var _error := ""

func get_error() -> String:
	return _error

func is_ok() -> bool:
	return _error == "" and not _rpn.is_empty()

# ---------------------------------------------------------------- 编译

static func compile(expr: String) -> GwExpr:
	var self_ := GwExpr.new()
	self_._compile(expr)
	return self_

func _compile(expr: String) -> void:
	_rpn.clear()
	_error = ""
	var toks := _tokenize(expr)
	if _error != "":
		return
	# 隐式乘法：`)x`、`2x`、`x sin(x)`、`)(` 之间补 `*`（内核支持隐式乘法，docs/03）。
	# 注意 `sin(` 里的 `sin` 是**函数名**，它后面不能补 `*`，否则整个表达式就废了
	# （这个 bug 真出现过：`sin(x)` 被解析成 `sin * (x)`）。
	var fixed: Array = []
	for i in toks.size():
		fixed.append(toks[i])
		if i + 1 >= toks.size():
			continue
		var a: Dictionary = toks[i]
		var b: Dictionary = toks[i + 1]
		var a_is_value: bool = (
			a.kind == "num" or a.kind == "rparen"
			or (a.kind == "ident" and not (FUNCS.has(String(a.text)) and b.kind == "lparen"))
		)
		var b_starts_value: bool = b.kind == "num" or b.kind == "lparen" or b.kind == "ident"
		if a_is_value and b_starts_value:
			fixed.append({"kind": "op", "text": "*"})
	_shunting_yard(fixed)

func _tokenize(expr: String) -> Array:
	var out: Array = []
	var i := 0
	var n := expr.length()
	while i < n:
		var ch := expr[i]
		if ch == " " or ch == "\t" or ch == "\n":
			i += 1
			continue
		if ch == "(":
			out.append({"kind": "lparen", "text": "("})
			i += 1
			continue
		if ch == ")":
			out.append({"kind": "rparen", "text": ")"})
			i += 1
			continue
		if ch.is_valid_int() or ch == ".":
			var start := i
			while i < n and (expr[i].is_valid_int() or expr[i] == "."):
				i += 1
			var txt := expr.substr(start, i - start)
			if not txt.is_valid_float():
				_error = "非法数字：%s" % txt
				return []
			out.append({"kind": "num", "value": txt.to_float()})
			continue
		if "+-*/^".contains(ch):
			out.append({"kind": "op", "text": ch})
			i += 1
			continue
		if ch.is_valid_identifier() or ch == "_":
			var s2 := i
			while i < n and (expr[i].is_valid_identifier() or expr[i].is_valid_int() or expr[i] == "_"):
				i += 1
			out.append({"kind": "ident", "text": expr.substr(s2, i - s2)})
			continue
		if ch == "π":
			out.append({"kind": "num", "value": PI})
			i += 1
			continue
		_error = "不认识的字符：%s" % ch
		return []
	return out

func _prec(op: String) -> int:
	match op:
		"u": return 3          # 一元负号
		"^": return 4
		"*", "/": return 2
		"+", "-": return 1
	return 0

func _shunting_yard(toks: Array) -> void:
	var stack: Array = []
	var prev_significant := ""      # 用于判断一元负号
	for t in toks:
		match t.kind:
			"num":
				_rpn.append({"op": "num", "value": t.value})
			"ident":
				var name: String = t.text
				if name == "x":
					_rpn.append({"op": "x"})
				elif name == "pi" or name == "π" or name == "n":
					_rpn.append({"op": "num", "value": PI})
				elif name == "e":
					_rpn.append({"op": "num", "value": 2.718281828459045})   # Godot 没有内建 E
				elif FUNCS.has(name):
					stack.append({"kind": "func", "text": name})
				else:
					_error = "未知标识符：%s" % name
					return
			"lparen":
				stack.append({"kind": "lparen", "text": "("})
			"rparen":
				while not stack.is_empty() and stack.back().kind != "lparen":
					_rpn.append(_pop(stack))
				if stack.is_empty():
					_error = "括号不匹配"
					return
				stack.pop_back()                              # 丢掉 "("
				if not stack.is_empty() and stack.back().kind == "func":
					_rpn.append({"op": "func", "name": stack.pop_back().text})
			"op":
				var op: String = t.text
				if op == "-" and (prev_significant == "" or prev_significant == "(" or prev_significant == "op"):
					op = "u"
				while not stack.is_empty() and stack.back().kind == "op":
					var top: String = stack.back().text
					var right_assoc := top == "u"
					if _prec(top) > _prec(op) or (_prec(top) == _prec(op) and not right_assoc):
						_rpn.append(_pop(stack))
					else:
						break
				stack.append({"kind": "op", "text": op})
		prev_significant = "(" if t.kind == "lparen" else ("op" if t.kind == "op" else "val")
	while not stack.is_empty():
		if stack.back().kind == "lparen":
			_error = "括号不匹配"
			return
		_rpn.append(_pop(stack))
	if _rpn.is_empty():
		_error = "空表达式"

func _pop(stack: Array) -> Dictionary:
	var d: Dictionary = stack.pop_back()
	if d.kind == "op":
		return {"op": d.text}
	return {"op": "func", "name": d.text}

# ---------------------------------------------------------------- 求值

## 在 x = xv 处求值。出错返回 NAN。
func eval(xv: float) -> float:
	if not is_ok():
		return NAN
	var st: Array[float] = []
	for item in _rpn:
		match String(item.op):
			"num":
				st.append(float(item.value))
			"x":
				st.append(xv)
			"func":
				if st.is_empty():
					return NAN
				var a: float = st.pop_back()
				st.append(_apply_func(String(item.name), a))
			"u":
				if st.is_empty():
					return NAN
				st.append(-st.pop_back())
			"+", "-", "*", "/", "^":
				if st.size() < 2:
					return NAN
				var b: float = st.pop_back()
				var a2: float = st.pop_back()
				st.append(_apply_op(String(item.op), a2, b))
			_:
				return NAN
	if st.size() != 1:
		return NAN
	return st.back()

static func _apply_func(name: String, a: float) -> float:
	match name:
		"sin": return sin(a)
		"cos": return cos(a)
		"tan": return tan(a)
		"sqrt": return sqrt(a) if a >= 0.0 else NAN
		"abs": return absf(a)
		"ln": return log(a) if a > 0.0 else NAN
		"log": return log(a) / log(10.0) if a > 0.0 else NAN   # 内核的 log 是 log10（docs/03）
		"exp": return exp(a)
	return NAN

static func _apply_op(op: String, a: float, b: float) -> float:
	match op:
		"+": return a + b
		"-": return a - b
		"*": return a * b
		"/": return a / b if b != 0.0 else NAN
		"^": return pow(a, b)
	return NAN
