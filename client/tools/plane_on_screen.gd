extends Control
## **真 `GraphwarPlane` 出现在播放屏上的最小场景**（`docs/16-client.md` §43）。
##
## 这个文件不是游戏逻辑，也不改客户端任何现有界面：它是**接线演示 + 验收工具**
## （与 `selftest.gd` / `curve_on_screen_check.gd` 一样住在 `tools/`，不进运行时路径）。
##
## # 它证明什么
##
## 场景里那一个 `GraphwarPlane` 节点是 **Rust 侧的扩展类**（`src/_scaffold/graphwar_plane_class.rs`），
## 不是 GDScript 仿制品。它进树后自己建 3 个 `FunctionDrawer` 子节点（§40），本脚本只做
## 原版播放屏该做的三件事：
##
## ```text
## ① 拿一条**真实弹道**的点序列      —— 内核 `GW2Rebuild.path_calculate()`（复原的
##                                    `graphwar_shot_calculator`），不是手画抛物线；
## ② 把点交给平面（原版
##    `GraphwarFunctionManager::set_function_points` + 队色）；
## ③ 每帧推进 tick                   —— 原版 `GraphwarTickHandler::get_fractional_tick`，
##                                    平面在 `_draw` 里按 30 点/tick 长曲线（§42）。
## ```
##
## 之后的一切都发生在 Rust 类里：`_process` 判"还在揭示"就 `queue_redraw` →
## 引擎调 `_draw` → 画棋盘框 + `feed_functions()` → `FunctionDrawer::update_function`
## → `Line2D::add_point`。GDScript 这一侧**一个图元都不画**。
##
## # 弹道点从哪来（档位写清，别当成"原版真值"）
##
## | 项 | 值 | 档位 |
## |---|---|---|
## | 函数文本 | `"cos(x)"` | **观测**：`fixtures/protocol_match.jsonl` 第 20 行
##   `{"FunctionUpdateRequest":{"player_id":232,"function":"cos(x)"}}`（真实抓包） |
## | player id | `232` | **观测**：同一份抓包第 21 行 `{"FunctionFireRequest":{"player_id":232}}` |
## | 起点 / 方向 | `(-18, -5.1)` / `+1` | **我方选择**：抓包的开火报文**只有** player_id，
##   坐标与方向在客户端（士兵位置）⇒ 这两个值没有报文证据 |
## | 点序列本身 | 5314 个点（本函数上游内核算出） | **推断**：算法是复原的
##   `graphwar_shot_calculator`/`graphwar_path_calculator`（`cargo test` 与 C ABI 同口径），
##   但**原版运行时**的点我们没抓到 ⇒ 它是"复原算法产出的真实弹道"，不是原版逐位真值 |
##
## ⚠️ 精度：内核 `path_calculate` 的返回值是 `PackedVector2Array`（f32），本脚本再摊回 f64
## 交给 `set_function_points` —— 这一次窄化发生在**内核 API 边界**，与 `FunctionDrawer`
## 里 `cvtsd2ss`（§40.4b）是同一处掉队，不额外丢精度。
##
## # 屏幕上的位置（`update_transform` 的实参是**我方选择**）
##
## 原版的 `FunctionDrawer::update_transform`（`0x499050`）调用点在 `GraphwarPlane::draw`
## 里（`0x4b6147`），三个实参只解出一半（§42.5 第 4 条）⇒ 本类**没有**接。
## 但 `Line2D` 的点是**棋盘坐标**（`x ∈ [-25,25]`、`y ∈ [-15,15]`），不摆位置就是挤在
## 原点附近 50×30 像素的一团。本场景按 `graphwar_plane::to_pixel` 那套换算摆一次：
##
## ```text
## position = PIXEL_OFFSET = (500, 300)      ; 像素 = 20*棋盘 + (500,300)
## scale    = (20, -20)                       ; 20 = PX_PER_UNIT，负号 = 棋盘 y 向上
## ⇒ 点 (x,y) 落在 (500+20x, 300-20y)，与 to_pixel 逐点相同
## ```
##
## `set_width(0.125)`（逐位常量，§40.3b）在 scale=20 下就是 **2.5 像素**宽
## —— 这反过来解释了 0.125 为什么这么小：它是**棋盘单位**。
##
## # 怎么跑
##
## ```text
## bash client/run.sh selftest                                    # 不许退（回归）
## "$GODOT" --headless --path client --script res://tools/plane_on_screen_check.gd   # 验收（30+ 条断言）
## # 有显示器的截图（可选证据，产物是 gitignored 的）：
## xvfb-run -a "$GODOT" --path client --rendering-driver opengl3 --resolution 1600x900 \
##     res://tools/plane_on_screen.tscn -- --shot=/tmp/plane_on_screen.png
## ```

const KERNEL_CLASS := "GW2Rebuild"
const PLANE_CLASS := "GraphwarPlane"
const DRAWER_CLASS := "FunctionDrawer"
const SCENE_PATH := "res://tools/plane_on_screen.tscn"

## 真实抓包里的函数文本（`fixtures/protocol_match.jsonl` 第 20 行）。
const FUNCTION_TEXT := "cos(x)"
## 真实抓包里的开火 player id（同一份文件第 21 行）。
const PLAYER_ID := 232
## 起点 / 方向：抓包的开火报文只有 player_id ⇒ **我方选择**（见文件头）。
const ORIGIN := Vector2(-18.0, -5.1)
const DIRECTION := 1.0
## 起飞 tick（原版 `GraphwarFunctionManager` 的 value+0x74）。本演示从 0 起 —— **我方选择**
## （真实值随对局走，抓包里没有 tick 号）。
const ACTIVE_TICK := 0
## 队色 `0x00RRGGBB`，**低字节 = R**（§40.3h）。`#7D6800` = `Color8(125,104,0)`：
## 真帧量测出来的弹道色（`docs/16-client.md` §12.2 ⑫），= `TEAM_COLORS[0][2]`。
const TEAM_COLOR := 0x00_00_68_7D
## 每 tick 露几个点：`.rodata 0x19ef90` = f64 30.0（§42.2，逐位）。这里只用它算"几 tick 揭完"，
## 真正的揭示由 Rust 侧的 `revealed_points()` 做，GDScript 不重写那个判定。
const POINTS_PER_TICK := 30.0
## `graphwar_plane::PX_PER_UNIT` = CANVAS_W / BOARD_W = 1000 / 50。
const PX_PER_UNIT := 20.0
## `graphwar_plane::PIXEL_OFFSET` = (500, 300)（`to_pixel` 的加法项，逐位）。
const PIXEL_OFFSET := Vector2(500.0, 300.0)
## 自动播放时每帧推进几个 tick（**我方选择**：只影响看的速度，不影响最终点数）。
const AUTOPLAY_TICKS_PER_FRAME := 4.0

## 自动推进 tick（GUI / 截图用）。验收脚本在 `add_child` **之前**把它设成 false，自己按 tick 驱动。
@export var autoplay := true

## 截图模式：非空 ⇒ 曲线揭完、画面稳定后存 PNG 并退出（`--shot=<path>` 设置）。
var capture_path := ""

var _plane: Control
var _drawer: Control
var _kernel: Object
var _points: PackedFloat64Array
var _total := 0
var _tick := 0.0
var _install_error := ""
var _transform_done := false
var _settle_frames := 0
var _shot_done := false

# ---------------------------------------------------------------------------
# 进树：接线（原版播放屏该做的三件事）
# ---------------------------------------------------------------------------

func _ready() -> void:
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--shot="):
			capture_path = arg.substr("--shot=".length())
	# 视口一旦不是 1600×900，棋盘就按比例居中（`GraphwarPlane` 自己画 1000×600）。
	# 原版窗口是 `MODE_MAXIMIZED`（project.godot 的 display/window/size/mode=2），
	# 首帧之后还会变一次尺寸 ⇒ 跟着 `size_changed` 再居中一次。
	_center_plane()
	get_viewport().size_changed.connect(_center_plane)
	install()

## 把平面摆到视口中央（棋盘本体固定 1000×600 画布像素：`BOARD_RECT` × `PX_PER_UNIT`）。
func _center_plane() -> void:
	_plane = get_node_or_null("GraphwarPlane") as Control
	if _plane == null:
		_install_error = "场景里没有 GraphwarPlane 节点"
		return
	var vp := get_viewport_rect().size
	# `maxf(0, …)`：`--headless` 下视口只有 64×64（没有真窗口），居中会算出负坐标；
	# 那个模式只跑断言、不出图，所以直接靠左上（棋盘本体仍画在 (0,0)-(1000,600)）。
	_plane.position = Vector2(
		maxf(0.0, roundf((vp.x - 50.0 * PX_PER_UNIT) * 0.5)),
		maxf(0.0, roundf((vp.y - 30.0 * PX_PER_UNIT) * 0.5)))

## 接线的本体：算弹道 → 交给平面 → 推到 tick=1（原版 tick 由 `GraphwarTickHandler` 给）。
##
## 返回是否接上。失败原因放 `install_error()`（headless 里要能分清"内核没加载"和"场景没接上"）。
func install() -> bool:
	_install_error = ""
	if _plane == null:
		if _install_error == "":
			_install_error = "场景里没有 GraphwarPlane 节点"
		return false
	if not ClassDB.class_exists(KERNEL_CLASS):
		_install_error = "内核未加载（ClassDB 里没有 %s）" % KERNEL_CLASS
		return false
	if not ClassDB.class_exists(PLANE_CLASS):
		_install_error = "扩展类未加载（ClassDB 里没有 %s）" % PLANE_CLASS
		return false

	# ① 真实弹道：复原的 `graphwar_shot_calculator`（= `GW2Rebuild.path_calculate`）。
	_kernel = ClassDB.instantiate(KERNEL_CLASS)
	var result: Dictionary = _kernel.call(
		"path_calculate", FUNCTION_TEXT, ORIGIN.x, ORIGIN.y, DIRECTION)
	if not bool(result.get("ok", false)):
		_install_error = "内核算不出弹道：%s" % str(result.get("error", "?"))
		return false
	var pts: PackedVector2Array = result.get("points", PackedVector2Array())
	_total = pts.size()
	if _total < 2:
		_install_error = "内核只给出 %d 个点" % _total
		return false

	# ② 交给平面：原版是 `GraphwarFunctionManager::set_function_points`（`0x456810`）+
	#    active_tick（`0x456b80`）+ 队色（`get_team_overview`，`0x4be6e0`）。
	#    点的载体是**扁平 f64**（原版一个点是两个 f64，§42.4）。
	_points = PackedFloat64Array()
	_points.resize(_total * 2)
	for i in _total:
		_points[i * 2] = pts[i].x
		_points[i * 2 + 1] = pts[i].y
	_plane.call("build_children")
	_plane.call("set_function_points", PLAYER_ID, _points, ACTIVE_TICK, TEAM_COLOR)

	# ③ 推到第一个"在场"的 tick（原版 `active_tick < tick` 才在场，§42.2 的 `jbe`）。
	_tick = float(ACTIVE_TICK) + 1.0
	_plane.call("set_fractional_tick", _tick)
	return true

# ---------------------------------------------------------------------------
# 每帧：推进 tick（几何/绘制全在 Rust 类里）
# ---------------------------------------------------------------------------

func _process(_delta: float) -> void:
	if _plane == null or _total == 0:
		return

	# `update_transform` 只对**已经建出来**的 `Line2D` 生效，所以等第一条线出现再摆一次
	# （原版这个调用在 `GraphwarPlane::draw` 里，`0x4b6147`，本类尚未接 —— §42.5 第 4 条）。
	_drawer = _plane.get_child(0) if _plane.get_child_count() > 0 else null
	if not _transform_done and _drawer != null and int(_drawer.call("function_count")) > 0:
		_drawer.call("update_transform", PIXEL_OFFSET.x, PIXEL_OFFSET.y,
			Vector2(PX_PER_UNIT, -PX_PER_UNIT), false)
		_transform_done = true

	if autoplay and _tick < final_tick():
		_tick = minf(_tick + AUTOPLAY_TICKS_PER_FRAME, final_tick())
		_plane.call("set_fractional_tick", _tick)

	if capture_path != "" and not _shot_done and is_fully_revealed():
		_settle_frames += 1
		if _settle_frames >= 2:
			_shot_done = true
			_capture_and_quit()

# ---------------------------------------------------------------------------
# 验收脚本用的驱动口（确定性：不靠帧数）
# ---------------------------------------------------------------------------

## 把 tick 推到 `t`（只增不减）。
func advance_to(t: float) -> void:
	_tick = maxf(_tick, t)
	if _plane != null:
		_plane.call("set_fractional_tick", _tick)

## 揭完需要到哪个 tick：`ceil(点数 / 30) + active_tick`。
func final_tick() -> float:
	if _total <= 0:
		return 0.0
	return ceilf(float(_total) / POINTS_PER_TICK) + float(ACTIVE_TICK)

## 曲线是否已经全揭出来（`Line2D` 上的点数 == 喂进去的点数）。
func is_fully_revealed() -> bool:
	if _drawer == null:
		return false
	return int(_drawer.call("point_count", PLAYER_ID)) >= _total

func install_error() -> String:
	return _install_error

func total_points() -> int:
	return _total

func function_text() -> String:
	return FUNCTION_TEXT

func player_id() -> int:
	return PLAYER_ID

func plane() -> Control:
	return _plane

func drawer() -> Control:
	return _drawer

## 一条记录（验收脚本打印/断言用；键名就是文档里那张表的行）。
func report() -> Dictionary:
	var line_points := -1
	var written := -1
	var add_points := -1
	var line_instance := -1
	if _drawer != null:
		line_points = int(_drawer.call("point_count", PLAYER_ID))
		written = int(_drawer.call("written", PLAYER_ID))
		add_points = int(_drawer.call("add_point_total"))
		line_instance = int(_drawer.call("line_instance_id", PLAYER_ID))
	var out := {
		"kernel": KERNEL_CLASS,
		"kernel_ok": _install_error == "",
		"install_error": _install_error,
		"function": FUNCTION_TEXT,
		"player_id": PLAYER_ID,
		"fed_points": _total,
		"line_points": line_points,
		"drawer_written": written,
		"add_point_total": add_points,
		"line_instance_id": line_instance,
		"tick": _tick,
		"final_tick": final_tick(),
		"fully_revealed": is_fully_revealed(),
	}
	if _plane != null:
		out["plane_class"] = _plane.get_class()
		out["plane_draw_calls"] = int(_plane.call("draw_call_count"))
		out["plane_rect_total"] = int(_plane.call("drawn_rect_total"))
		out["plane_process_calls"] = int(_plane.call("process_call_count"))
		out["plane_ready_calls"] = int(_plane.call("ready_call_count"))
		out["function_write_total"] = int(_plane.call("function_write_total"))
		out["function_point_total"] = int(_plane.call("function_point_total"))
		out["reveal_redraw_count"] = int(_plane.call("reveal_redraw_count"))
		out["function_drawer_count"] = int(_plane.call("function_drawer_count"))
	return out

# ---------------------------------------------------------------------------
# 截图（可选证据）：存 PNG + 数一遍棋盘区域里有哪些颜色
# ---------------------------------------------------------------------------

func _capture_and_quit() -> void:
	if DisplayServer.get_name() == "headless":
		print("SCREENSHOT_SKIPPED headless 没有渲染器（用 xvfb-run + --rendering-driver opengl3）")
		get_tree().quit(0)
		return
	var dir_path := capture_path.get_base_dir()
	if dir_path != "":
		DirAccess.make_dir_recursive_absolute(dir_path)
	var img := get_viewport().get_texture().get_image() if get_viewport() != null else null
	if img == null:
		print("SCREENSHOT_SKIPPED 拿不到视口纹理")
		get_tree().quit(0)
		return
	var err := img.save_png(capture_path)
	var stats := _board_color_stats(img)
	print("SCREENSHOT path=%s err=%d size=%dx%d samples=%d line_pixels=%d bg_pixels=%d"
		% [capture_path, err, img.get_width(), img.get_height(),
			stats["samples"], stats["line"], stats["bg"]])
	print("SCREENSHOT_TOP_COLORS %s" % str(stats["top"]))
	# 退出前把内核对象松掉（RefCounted；不松 Godot 会在退出时报 "ObjectDB instances leaked"）。
	_kernel = null
	get_tree().quit(0)

## 棋盘区域（平面节点的全局矩形）里按 2 像素步长采样，数"接近队色"与"接近底色"的点，
## 再给出出现最多的 6 种颜色。用途只有一个：证明**曲线上屏了**（不是只涨计数器）。
func _board_color_stats(img: Image) -> Dictionary:
	var line_color := Color(
		float(TEAM_COLOR & 0xff) / 255.0,
		float((TEAM_COLOR >> 8) & 0xff) / 255.0,
		float((TEAM_COLOR >> 16) & 0xff) / 255.0)
	var bg := Color(0.054902, 0.145098, 0.109804)
	var origin := Vector2i(_plane.global_position.round())
	var w := int(50.0 * PX_PER_UNIT)
	var h := int(30.0 * PX_PER_UNIT)
	var samples := 0
	var line := 0
	var bg_px := 0
	var hist := {}
	var x := 0
	while x < w:
		var y := 0
		while y < h:
			var px := origin + Vector2i(x, y)
			if px.x >= 0 and px.y >= 0 and px.x < img.get_width() and px.y < img.get_height():
				var c := img.get_pixel(px.x, px.y)
				samples += 1
				if _color_close(c, line_color, 0.12):
					line += 1
				if _color_close(c, bg, 0.03):
					bg_px += 1
				var key := "%02x%02x%02x" % [int(c.r * 255.0), int(c.g * 255.0), int(c.b * 255.0)]
				hist[key] = int(hist.get(key, 0)) + 1
			y += 2
		x += 2
	var pairs: Array = []
	for k in hist:
		pairs.append([k, hist[k]])
	pairs.sort_custom(func(a, b): return a[1] > b[1])
	var top: Array = []
	for i in mini(6, pairs.size()):
		top.append("%s=%d" % [pairs[i][0], pairs[i][1]])
	return {"samples": samples, "line": line, "bg": bg_px, "top": top}

func _color_close(a: Color, b: Color, tol: float) -> bool:
	return absf(a.r - b.r) <= tol and absf(a.g - b.g) <= tol and absf(a.b - b.b) <= tol
