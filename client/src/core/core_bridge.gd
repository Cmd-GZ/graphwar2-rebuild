extends RefCounted
class_name GwCore
## 内核桥：界面 → Rust 复原内核。
##
## 现状（`src/gdext_class.rs` 观测）：GDExtension 入口已经注册了类 `GW2Rebuild`
## （父类 `RefCounted`），但**只注册了一个方法 `ping()`**。`path::calculate`、
## `shot`、`life`、`turn` 都还没向 GDScript 暴露。
## 因此本文件是**明写的接缝**：
##   1. 如果内核在，就用内核；
##   2. 不在（或没有该方法），退到 `preview` —— 固定 dx 折线的**界面预览**，不是采样器；
##   3. 界面必须把 `source` 字段显示出来（PlayScreen 底部的状态行），
##      不允许把预览当成"内核算出来的弹道"。
##
## 接入真内核时只需要在内核侧加一个方法，约定如下（推荐签名）：
##     GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float)
##         -> Dictionary { ok: bool, points: PackedVector2Array, offset: float, error: String }
## 参数与返回值与本文件 `calculate_path()` 完全一致，调用方无需改动。

## 棋盘边界（`AGENTS.md` 的硬事实：`|x| <= 25`、`|y| <= 15`，越界点在**接受之前**检查）。
const MAX_ABS_X := 25.0
const MAX_ABS_Y := 15.0

## 内核采样步长（弦长）—— 只做展示，本文件不用它采样；留在这里是为了在界面上
## 显示"真实步长是 0.01"这条事实（`docs/89`）。
const KERNEL_STEP := 0.01

## 预览折线的 x 步长（**不是**内核的弦长）。
const PREVIEW_DX := 0.02
const PREVIEW_MAX_POINTS := 4000

const KERNEL_CLASS := "GW2Rebuild"

static var _kernel_checked := false
static var _kernel_ok := false
static var _kernel_methods: PackedStringArray = PackedStringArray()
static var _kernel_note := ""

static func kernel_available() -> bool:
	_probe_kernel()
	return _kernel_ok

static func kernel_note() -> String:
	_probe_kernel()
	return _kernel_note

static func kernel_methods() -> PackedStringArray:
	_probe_kernel()
	return _kernel_methods

static func _probe_kernel() -> void:
	if _kernel_checked:
		return
	_kernel_checked = true
	if not ClassDB.class_exists(KERNEL_CLASS):
		_kernel_ok = false
		_kernel_note = "内核未加载（ClassDB 里没有 %s）→ 弹道用界面预览" % KERNEL_CLASS
		return
	var obj: Object = ClassDB.instantiate(KERNEL_CLASS)
	if obj == null:
		_kernel_ok = false
		_kernel_note = "内核类存在但实例化失败"
		return
	var names := PackedStringArray()
	for m in obj.get_method_list():
		var n := String(m.name)
		if not n.begins_with("_"):
			names.append(n)
	_kernel_methods = names
	if names.has("path_calculate"):
		_kernel_ok = true
		_kernel_note = "内核 %s 已加载，使用 path_calculate" % KERNEL_CLASS
	else:
		_kernel_ok = false
		_kernel_note = ("内核 %s 已加载，但只暴露 %s —— 弹道用界面预览"
			% [KERNEL_CLASS, ", ".join(names) if names.size() > 0 else "(无方法)"])

## 算一条弹道。
## start: 士兵所在位置（棋盘坐标）；direction: +1 向右飞（team 0），-1 向左飞（team 1）。
## 返回 { ok, points: PackedVector2Array, offset: float, source: String, note: String }
static func calculate_path(expr_text: String, start: Vector2, direction: float) -> Dictionary:
	var expr := GwExpr.compile(expr_text)
	if not expr.is_ok():
		return {
			"ok": false, "points": PackedVector2Array(), "offset": NAN,
			"source": "parse", "note": "函数解析失败：%s" % expr.get_error(),
		}
	_probe_kernel()
	if _kernel_ok:
		var obj: Object = ClassDB.instantiate(KERNEL_CLASS)
		if obj != null and obj.has_method("path_calculate"):
			var res: Variant = obj.call("path_calculate", expr_text, start.x, start.y, direction)
			if res is Dictionary and bool(res.get("ok", false)):
				var pts: PackedVector2Array = res.get("points", PackedVector2Array())
				return {
					"ok": true, "points": pts,
					"offset": float(res.get("offset", 0.0)),
					"source": "kernel", "note": "内核 path_calculate",
				}
	return _preview(expr, start, direction)

## 界面预览：固定 dx 折线 + 修正常数。**不是**内核的等弦长采样。
static func _preview(expr: GwExpr, start: Vector2, direction: float) -> Dictionary:
	var f0 := expr.eval(start.x)
	if is_nan(f0):
		return {"ok": false, "points": PackedVector2Array(), "offset": NAN,
			"source": "preview", "note": "在起点 x=%.3f 处函数无定义" % start.x}
	# 修正常数：c = y0 - f(x0)（内核侧同式，见 docs/shot.cs 与 docs/105 的 flip 说明）
	var offset := start.y - f0
	var pts := PackedVector2Array()
	pts.append(start)
	var dx := PREVIEW_DX * (1.0 if direction >= 0.0 else -1.0)
	var x := start.x
	var reason := "point_budget"
	while pts.size() < PREVIEW_MAX_POINTS:
		x += dx
		if absf(x) > MAX_ABS_X:
			reason = "bounds_x"
			break
		var y := expr.eval(x) + offset
		if is_nan(y):
			reason = "undefined"
			break
		if absf(y) > MAX_ABS_Y:
			reason = "bounds_y"
			break
		pts.append(Vector2(x, y))
	return {
		"ok": pts.size() > 1,
		"points": pts,
		"offset": offset,
		"source": "preview",
		"note": "界面预览（固定 dx=%.3f 折线，非内核等弦长采样）：%s" % [PREVIEW_DX, reason],
	}

## 修正常数的显示格式：原版是 ` + 2.515 `（3 位小数，带符号与两侧空格；
## DiffEq 模式下为空，见 docs/92 §4.10）。
static func format_offset(offset: float) -> String:
	if is_nan(offset):
		return ""
	return " %+.3f " % offset
