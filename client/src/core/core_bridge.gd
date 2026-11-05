extends RefCounted
class_name GwCore
## 内核桥：界面 → Rust 复原内核。
##
## 现状（`src/gdext_class.rs` 观测）：GDExtension 入口注册了类 `GW2Rebuild`，
## 暴露两个方法：`path_calculate()`（弹道，`docs/116`）与 `hit_test()`（命中，`docs/120`）。
## 本文件是**明写的接缝**：
##   1. 如果内核在，就用内核；
##   2. 不在（或没有该方法），退到 `preview` —— 弹道退到固定 dx 折线的**界面预览**，
##      命中退到"路径点到士兵中心 < 半径"的**几何近似**，两者都不是复原出来的规则；
##   3. 界面必须把 `source` 字段显示出来（PlayScreen 底部的状态行），
##      不允许把预览/近似当成"内核算出来的结果"。
##
## 接入真内核时只需要在内核侧加一个方法，约定如下（推荐签名）：
##     GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float)
##         -> Dictionary { ok: bool, points: PackedVector2Array, offset: float, error: String }
## 参数与返回值与本文件 `calculate_path()` 完全一致，调用方无需改动。
##
## 命中侧的契约（`docs/120` §1，实现见内核 `src/api.rs::hit_test`）：
##     GW2Rebuild.hit_test(points: PackedVector2Array,
##                         obstacles: PackedFloat64Array,   # [x, y, size] * n
##                         soldiers:  PackedFloat64Array)   # [x, y, radius] * n
##         -> Dictionary { ok: bool, hits: Array[int], hit_points: PackedVector2Array,
##                         first_hit: int, stop: Vector2, blocked: bool, error: String }
## 坐标一律**棋盘坐标**（`|x| <= 25`、`|y| <= 15`），像素换算在核心里做一次。

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

## 致死半径：抓包里 `PosInfo.radius` 恒为 **0.5**（`docs/24` §、`docs/25` §1、`docs/30` §3）。
## 注意与 `GwPlaneView.SOLDIER_RADIUS = 0.8` 区分：那个是**画多大**，这个是**打多准**。
const SOLDIER_HIT_RADIUS := 0.5

static var _kernel_checked := false
static var _kernel_ok := false
static var _kernel_has_hit_test := false
static var _kernel_methods: PackedStringArray = PackedStringArray()
static var _kernel_note := ""

static func kernel_available() -> bool:
	_probe_kernel()
	return _kernel_ok

## 命中判定能不能走内核（`hit_test` 是否注册上了）。
static func kernel_has_hit_test() -> bool:
	_probe_kernel()
	return _kernel_ok and _kernel_has_hit_test

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
	_kernel_has_hit_test = names.has("hit_test")
	if names.has("path_calculate"):
		_kernel_ok = true
		_kernel_note = "内核 %s 已加载，使用 path_calculate" % KERNEL_CLASS
		_kernel_note += (" + hit_test" if _kernel_has_hit_test else "（无 hit_test）")
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

## 判定一次开火的命中（`docs/120`）。
##
## 用内核的**掩膜规则**：障碍光栅化进 1000×600 掩膜决定弹道停在哪，士兵按
## `半径内经过即死` 单独判（`docs/30`：士兵**不**挡弹道）。调用方只要给出
## 弹道点与"谁在场"，不需要知道掩膜与像素换算。
##
## `points`：棋盘坐标的点序列（就是 `calculate_path()` 返回的 `points`，别做变换）。
## `soldiers`：`Array[Dictionary]`，每项 `{pos: Vector2, radius: float}`；
##             下标即返回值 `hits` 里的编号。**射手本人不要放进来**（`settle` 的约定）。
## `obstacles`：`Array[Dictionary]`，每项 `{pos: Vector2, size: float}`；可省。
##
## 返回 `{ ok, hits: Array[int], hit_points: PackedVector2Array, first_hit: int,
##         stop: Vector2, blocked: bool, source: String, note: String }`
## `source`：`kernel` = 内核掩膜判定；`preview` = 界面几何近似（内核不可用时）；
## `none` = 连近似都做不了（没有弹道点）。调用方必须把 `source` 显示出来。
static func hit_test(points: PackedVector2Array, soldiers: Array, obstacles: Array = []) -> Dictionary:
	if points.size() < 2:
		return {"ok": false, "hits": [], "hit_points": PackedVector2Array(),
			"first_hit": -1, "stop": points[0] if points.size() > 0 else Vector2.ZERO,
			"blocked": false, "source": "none", "note": "没有弹道点，无法判命中"}
	_probe_kernel()
	if kernel_has_hit_test():
		var obj: Object = ClassDB.instantiate(KERNEL_CLASS)
		if obj != null:
			var res: Variant = obj.call("hit_test", points,
				_flat_sizes(obstacles, "size", 0.0), _flat_sizes(soldiers, "radius", SOLDIER_HIT_RADIUS))
			if res is Dictionary and bool(res.get("ok", false)):
				var hits: Array[int] = []
				for v in res.get("hits", []):
					hits.append(int(v))
				return {
					"ok": true, "hits": hits,
					"hit_points": res.get("hit_points", PackedVector2Array()),
					"first_hit": int(res.get("first_hit", -1)),
					"stop": res.get("stop", Vector2.ZERO),
					"blocked": bool(res.get("blocked", false)),
					"source": "kernel",
					"note": "内核 hit_test（掩膜 %d×%d + 士兵半径）" % [1000, 600],
				}
			# 内核在、但这次调用失败：不静默降级 —— 把 error 带出去，由界面显示。
			var err := String(res.get("error", "unknown")) if res is Dictionary else "non_dict"
			return _approx_hits(points, soldiers, "内核 hit_test 失败(%s) → 几何近似" % err)
	return _approx_hits(points, soldiers, "内核无 hit_test → 几何近似")

## 几何近似（**不是**复原出来的规则，只是内核缺席时的占位）：
## 弹道任一点到士兵中心 < 半径即判死。陷阱：只看点、不看掩膜，也没有障碍。
static func _approx_hits(points: PackedVector2Array, soldiers: Array, note: String) -> Dictionary:
	var hits: Array[int] = []
	var hit_points := PackedVector2Array()
	for i in soldiers.size():
		var s: Dictionary = soldiers[i]
		var c: Vector2 = s.pos
		var r := float(s.get("radius", SOLDIER_HIT_RADIUS))
		for p in points:
			if p.distance_to(c) <= r:
				hits.append(i)
				hit_points.append(p)
				break
	return {
		"ok": true, "hits": hits, "hit_points": hit_points,
		"first_hit": -1, "stop": points[points.size() - 1],
		"blocked": false, "source": "preview", "note": note,
	}

## `[{pos: Vector2, size/radius: float}, ...]` → `PackedFloat64Array`，
## 步长 3，顺序是 `[x, y, size]`——与内核 `hit_test` 的入参约定一致（`docs/120` §1）。
## `fallback`：字典里没有该键时用的值（士兵 0.5，障碍 0.0）。
static func _flat_sizes(items: Array, key: String, fallback: float) -> PackedFloat64Array:
	var flat := PackedFloat64Array()
	flat.resize(items.size() * 3)
	for i in items.size():
		var it: Dictionary = items[i]
		var p: Vector2 = it.pos
		flat[i * 3 + 0] = p.x
		flat[i * 3 + 1] = p.y
		flat[i * 3 + 2] = float(it.get(key, fallback))
	return flat

## 修正常数的显示格式：原版是 ` + 2.515 `（3 位小数，带符号与两侧空格；
## DiffEq 模式下为空，见 docs/92 §4.10）。
static func format_offset(offset: float) -> String:
	if is_nan(offset):
		return ""
	# 观测格式（`docs/92` §4.10 + 原版真帧）：符号与数字之间**有一个空格**，
	# 三位小数，例：`+ 2.515`、`+ 16.872`、`- 8.315`（`docs/24` §6.2 的样本同形）。
	var sign_str := "+" if offset >= 0.0 else "-"
	return "%s %.3f" % [sign_str, absf(offset)]

