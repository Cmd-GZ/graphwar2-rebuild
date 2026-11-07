extends SceneTree
## **真 `GraphwarPlane` 在场景里把曲线画出来**的验收脚本（`docs/16-client.md` §43）。
##
## ```text
## cargo build --offline --features gdext-bindings
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## "$GODOT" --headless --path client --import
## "$GODOT" --headless --path client --script res://tools/plane_on_screen_check.gd ; echo "EXIT=$?"
## ```
##
## 它验的是**接线**，不是复原逻辑本身（后者的真值在反汇编里，`cargo test` 只是回归保护）：
##
## 1. `.tscn` 真的能加载，里面的 `GraphwarPlane` 是 **Rust 扩展类**（`get_class()` + `is_class`），
##    并且 `.tscn` 里写的两个 `#[export]` 属性（`board_scale` / `flip_y`）真的读回来了；
## 2. 喂进去的点序列是**真实弹道**——本脚本自己再向内核要一次（`GW2Rebuild.path_calculate`），
##    与场景喂的**点数逐个相等**，且首末点相同；内核 `ping()` 必须是 `pong:gdext`
##    （证明跑的是 gdext 内核，不是 `client/src/core/core_bridge.gd` 的预览折线）；
## 3. 曲线逐 tick 长出来：**本脚本一次 `feed_functions` 都不调**，点数只由 `_draw` 里的
##    原版喂点循环（`0x4b6339` → `0x4b69ec`）产生 ⇒ 点数增长本身就是"`_draw` 被调到"的证据；
## 4. 最终 `Line2D.get_point_count()` **== 喂进去的点数**（本轮的验收口径）；
## 5. `Line2D` 被摆到棋盘坐标系（`update_transform`）：点 (x,y) → (500+20x, 300-20y)，
##    且这条线**在视口内**（不是画到屏幕外）。
##
## 期望的揭示点数**不调** Rust 的 `revealed_points()`，而是照反汇编算式在 GDScript 里
## 独立重写（`0x4b67a0`–`0x4b6812`）；常量 `30.0`（`.rodata 0x19ef90`）也写死在这里，
## Rust 侧改常量这里会红。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const SCENE_PATH := "res://tools/plane_on_screen.tscn"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"
const PLANE := "GraphwarPlane"
const DRAWER := "FunctionDrawer"
const KERNEL := "GW2Rebuild"

## 原版每 tick 揭示的点数（`.rodata 0x19ef90` = f64 30.0）。
const POINTS_PER_TICK := 30.0
## 棋盘换到画布的换算（`graphwar_plane::to_pixel`）：`px = 20x + 500`、`py = 300 - 20y`。
const PX_PER_UNIT := 20.0
const PIXEL_OFFSET := Vector2(500.0, 300.0)
## 真帧量测出来的弹道色 `#7D6800`（`docs/16-client.md` §12.2 ⑫），打包成 `0x00RRGGBB`（低字节 = R）。
const TEAM_COLOR := 0x00_00_68_7D

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

# ---------------------------------------------------------------------------
# 独立重写：反汇编算式的 GDScript 版（**不调** Rust 的 revealed_points）
# ---------------------------------------------------------------------------

## `0x4b67a0`–`0x4b6812`：`min(len, trunc(30*(fractional_tick - active_tick)))`。
func _expected_revealed(frac_tick: float, active_tick: int, total: int) -> int:
	var d: float = (frac_tick - float(active_tick)) * POINTS_PER_TICK
	if d <= 0.0:
		return 0
	return int(min(float(total), floor(d)))

## 揭完需要到哪个 tick：`ceil(total / 30) + active_tick`。
func _expected_final_tick(total: int, active_tick: int) -> float:
	return ceil(float(total) / POINTS_PER_TICK) + float(active_tick)

# ---------------------------------------------------------------------------
# 小工具
# ---------------------------------------------------------------------------

func _method_names(obj: Object) -> PackedStringArray:
	var names := PackedStringArray()
	for m in obj.get_method_list():
		names.append(String(m.name))
	return names

func _kernel_path(kern: Object, expr: String, origin: Vector2, direction: float) -> PackedVector2Array:
	var r: Dictionary = kern.call("path_calculate", expr, origin.x, origin.y, direction)
	if not bool(r.get("ok", false)):
		return PackedVector2Array()
	return r.get("points", PackedVector2Array())

func _line_of(drawer: Control, player_id: int) -> Line2D:
	var want: int = int(drawer.call("line_instance_id", player_id))
	if want == -1:
		return null
	for i in range(drawer.get_child_count()):
		var kid: Node = drawer.get_child(i)
		if kid is Line2D and kid.get_instance_id() == want:
			return kid
	return null

## 棋盘坐标 → 画布像素（本脚本自己算一遍，用来证实"线在视口内"）。
func _board_to_canvas(p: Vector2, line: Line2D, plane: Control) -> Vector2:
	return plane.global_position + line.position + Vector2(
		p.x * line.scale.x, p.y * line.scale.y)

func _initialize() -> void:
	print("=== GraphwarPlane 进场景验收（docs/16-client.md §43）")

	# 0) 扩展：`--script` 模式下一般已经加载；没有就显式加载一次。
	if not ClassDB.class_exists(PLANE):
		print("      load_extension(%s) -> %d" % [EXTENSION_PATH,
			GDExtensionManager.load_extension(EXTENSION_PATH)])

	for c in [PLANE, DRAWER, KERNEL]:
		ok(ClassDB.class_exists(c), "ClassDB.class_exists(\"%s\")" % c)
	if not ClassDB.class_exists(PLANE) or not ClassDB.class_exists(KERNEL):
		_finish()
		return

	# 1) 内核：真实弹道（与场景用的是同一个入口）
	var kern: Object = ClassDB.instantiate(KERNEL)
	ok(str(kern.call("ping")) == "pong:gdext",
		"内核 ping() == \"pong:gdext\"（跑的是 gdext 内核，不是预览折线）")
	var expr := "cos(x)"          # fixtures/protocol_match.jsonl 第 20 行（真实抓包）
	var origin := Vector2(-18.0, -5.1)   # **我方选择**（抓包的开火报文不带坐标）
	var direction := 1.0
	var kernel_pts := _kernel_path(kern, expr, origin, direction)
	print("      内核弹道：expr=\"%s\" 起点=%s 方向=%.1f ⇒ %d 个点"
		% [expr, str(origin), direction, kernel_pts.size()])
	ok(kernel_pts.size() > 500, "内核给出真实弹道（%d 个点 > 500）" % kernel_pts.size())
	ok(kernel_pts.size() > 1 and kernel_pts[0].is_equal_approx(origin),
		"弹道首点 == 起点 %s" % str(origin))

	# 2) 场景：加载 + 两个 #[export] 属性
	var packed := load(SCENE_PATH)
	ok(packed != null, "load(\"%s\") 非空" % SCENE_PATH)
	if packed == null:
		_finish()
		return
	var scene: Node = packed.instantiate()
	ok(scene != null, "场景能实例化")
	if scene == null:
		_finish()
		return
	ok(scene is Control, "场景根是 Control（%s）" % scene.get_class())
	# 关掉自动推进（**必须在 add_child 之前**：`_ready` 里会接线并推到 tick=1）。
	scene.set("autoplay", false)
	var plane := scene.get_node_or_null("GraphwarPlane") as Control
	ok(plane != null, "场景里有名为 GraphwarPlane 的节点")
	if plane == null:
		_finish()
		return
	ok(plane.get_class() == PLANE, "那个节点是 Rust 扩展类（get_class() == %s）" % PLANE)
	ok(plane.is_class("Control"), "GraphwarPlane 是 Control（%s）" % plane.get_class())
	ok(absf(float(plane.get("board_scale")) - PX_PER_UNIT) < 1e-6,
		".tscn 里的 board_scale = 20.0 读回来了（%s）" % str(plane.get("board_scale")))
	ok(bool(plane.get("flip_y")), ".tscn 里的 flip_y = true 读回来了")
	var names := _method_names(plane)
	for w in ["draw_call_count", "drawn_rect_total", "set_function_points", "feed_functions",
			"function_point_total", "function_drawer_count", "process_call_count"]:
		ok(names.has(w), "GraphwarPlane 的方法 %s() 存在" % w)

	# 3) 进树 → 真的 `_ready` / `_draw` 路径
	root.add_child(scene)
	await process_frame
	await process_frame
	await process_frame
	ok(str(scene.call("install_error")) == "", "接线没有报错（%s）" % str(scene.call("install_error")))
	ok(int(scene.call("total_points")) == kernel_pts.size(),
		"场景喂进去的点数 == 内核弹道点数（%d）" % int(scene.call("total_points")))
	ok(int(plane.call("ready_call_count")) >= 1, "_ready 被引擎调到过")
	ok(int(plane.call("function_drawer_count")) == 3, "平面自己建了 3 个 FunctionDrawer 子节点")
	ok(plane.get_child_count() == 3, "子节点数 == 3")
	if plane.get_child_count() == 0:
		_finish()
		return
	var drawer := plane.get_child(0) as Control
	ok(drawer != null and drawer.get_class() == DRAWER,
		"第 0 个子节点是 FunctionDrawer（%s）" % (drawer.get_class() if drawer != null else "null"))
	if drawer == null:
		_finish()
		return

	# 4) 第一条线：颜色 / 位置 / 变换（`update_transform` 由场景按 to_pixel 摆的）
	var player_id := int(scene.call("player_id"))
	var line := _line_of(drawer, player_id)
	ok(line != null, "player %d 的 Line2D 建出来了（曲线载体）" % player_id)
	if line == null:
		_finish()
		return
	ok(line.get_class() == "Line2D", "它是引擎原生的 Line2D")
	ok(line.default_color.is_equal_approx(Color(
		125.0 / 255.0, 104.0 / 255.0, 0.0, 1.0)),
		"Line2D.default_color == #7D6800（真帧量测的队色，低字节 = R）")
	ok(line.position.is_equal_approx(PIXEL_OFFSET),
		"Line2D.position == (500, 300)（= PIXEL_OFFSET）")
	ok(line.scale.is_equal_approx(Vector2(PX_PER_UNIT, -PX_PER_UNIT)),
		"Line2D.scale == (20, -20)（棋盘 y 向上 ⇒ 取负）")
	ok(is_equal_approx(line.width, 0.125),
		"Line2D.width == 0.125（.rodata 0x1baff8 的逐位常量；scale=20 ⇒ 2.5 像素）")

	# 5) 逐 tick 揭示：**一次 feed_functions 都不调**，点数只由 `_draw` 里的喂点循环产生
	print("--- 逐 tick 揭示（_draw 里的喂点循环；本脚本不调 feed_functions）")
	var total := int(scene.call("total_points"))
	var active_tick := 0   # 场景里的 ACTIVE_TICK
	var draws_before := int(plane.call("draw_call_count"))
	ok(draws_before >= 1, "进树后 `_draw` 已经被调到（%d 次）" % draws_before)
	ok(int(plane.call("drawn_rect_total")) == draws_before,
		"每次 `_draw` 都画了一条棋盘框（draw_rect 累计 == draw 次数 == %d）" % draws_before)
	var want1 := _expected_revealed(1.0, active_tick, total)
	ok(want1 == 30, "独立算式：tick 1.0 ⇒ 30 点（30×1）")
	ok(line.get_point_count() == want1,
		"**tick 1.0：Line2D.get_point_count() == %d**" % line.get_point_count())
	ok(int(drawer.call("point_count", player_id)) == want1,
		"FunctionDrawer.point_count(%d) == %d" % [player_id, want1])
	ok(int(plane.call("function_point_total")) == want1,
		"平面的喂点累计 == %d（来自 `_draw`，脚本没手动喂）" % want1)

	var line_id := line.get_instance_id()
	var ticks: Array[float] = [2.0, 3.0, 10.0, 60.0, 177.0]
	for t in ticks:
		var before_draws := int(plane.call("draw_call_count"))
		scene.call("advance_to", t)
		await process_frame
		await process_frame
		var want := _expected_revealed(t, active_tick, total)
		var got := line.get_point_count()
		ok(got == want, "tick %.0f：Line2D 点数 %d == 独立算式 %d" % [t, got, want])
		ok(int(plane.call("draw_call_count")) > before_draws or got == total,
			"tick %.0f：`_draw` 又被调了（%d → %d）"
			% [t, before_draws, int(plane.call("draw_call_count"))])
		ok(line.get_instance_id() == line_id, "tick %.0f：还是同一条 Line2D（增量，不是重建）" % t)
	ok(int(drawer.call("written", player_id)) == _expected_revealed(177.0, active_tick, total),
		"value 里的已写入数跟着涨（%d）" % int(drawer.call("written", player_id)))
	ok(int(drawer.call("add_point_total")) == int(line.get_point_count()),
		"add_point 调用次数 == Line2D 点数（%d）" % int(drawer.call("add_point_total")))

	# 6) 揭完：点数 == 喂进去的点数（本轮验收口径）
	print("--- 揭完（tick = final_tick）")
	var final_tick := float(scene.call("final_tick"))
	ok(is_equal_approx(final_tick, _expected_final_tick(total, active_tick)),
		"场景的 final_tick() == 独立算式 ceil(%d/30) = %.0f" % [total, final_tick])
	scene.call("advance_to", final_tick)
	await process_frame
	await process_frame
	await process_frame
	var want_full := _expected_revealed(final_tick, active_tick, total)
	ok(want_full == total, "独立算式：tick %.0f ⇒ 封顶到全部 %d 点" % [final_tick, total])
	ok(line.get_point_count() == total,
		"**Line2D.get_point_count() == 喂进去的点数（%d == %d）**"
		% [line.get_point_count(), total])
	ok(bool(scene.call("is_fully_revealed")), "场景报告 is_fully_revealed() == true")
	ok(int(plane.call("function_point_total")) == total,
		"平面累计喂了 %d 个点（== 全部）" % int(plane.call("function_point_total")))

	# 点**内容**也要对：首末点与内核弹道一致（不是只把计数凑上去）
	ok(line.get_point_position(0).is_equal_approx(kernel_pts[0]),
		"第 0 个点 == 内核弹道首点 %s" % str(kernel_pts[0]))
	ok(line.get_point_position(total - 1).is_equal_approx(kernel_pts[total - 1]),
		"第 %d 个点 == 内核弹道末点 %s" % [total - 1, str(kernel_pts[total - 1])])
	var mid := total / 2
	ok(line.get_point_position(mid).is_equal_approx(kernel_pts[mid]),
		"第 %d 个点 == 内核弹道第 %d 点" % [mid, mid])

	# 7) 同一个 tick 再喂 ⇒ 一个点都不加（`append_window` 的 `ja`，§40.3）
	var draws_stable := int(plane.call("draw_call_count"))
	ok(int(plane.call("feed_functions")) == 0, "同一个 tick 手动再喂 ⇒ 追加 0 点")
	ok(line.get_point_count() == total, "点数仍是 %d" % total)
	await process_frame
	ok(int(plane.call("draw_call_count")) >= draws_stable,
		"揭完之后 `_draw` 不再被无脑刷（%d → %d）"
		% [draws_stable, int(plane.call("draw_call_count"))])

	# 8) 这条线真的被摆到棋盘坐标系里（不是画在原点附近的一团，也不是屏幕外）
	var vp := root.get_viewport().get_visible_rect()
	var board_w := 50.0 * PX_PER_UNIT
	var board_h := 30.0 * PX_PER_UNIT
	for k in [0, total / 2, total - 1]:
		var lp: Vector2 = line.get_point_position(k)
		var canvas := _board_to_canvas(lp, line, plane)
		# `to_pixel` 是 `(20x+500, 20y+300)`（**不翻 y**）；翻 y 的是 `draw_set_transform`
		# 的 scale（`flip_y`）。`Line2D` 这边由 `scale=(20,-20)` 做同一件事 ⇒ 它的 y 与
		# `to_pixel` 的 y 关于棋盘高（30×20 = 600）对称：`600 - to_pixel.y`。
		var want := Vector2(plane.call("to_pixel", lp.x, lp.y))
		want.y = board_h - want.y
		# 容差 1.5 px：`to_pixel` 返回 `Vector2i`（**截断**，不是四舍五入），差半像素是它的口径。
		ok((canvas - plane.global_position).distance_to(want) <= 1.5,
			"第 %d 点按 to_pixel+翻 y 落在 %s（线变换 => %s）" % [k, str(want), str(canvas - plane.global_position)])
		ok(canvas.x - plane.global_position.x >= 0.0 and canvas.x - plane.global_position.x <= board_w
			and canvas.y - plane.global_position.y >= 0.0
			and canvas.y - plane.global_position.y <= board_h,
			"第 %d 点落在 1000×600 的棋盘内（%s）"
			% [k, str(canvas - plane.global_position)])
	print("      视口=%s（--headless 下是 64×64：没有真窗口）平面位置=%s"
		% [str(vp.size), str(plane.position)])
	ok(plane.position.is_equal_approx(Vector2(
		maxf(0.0, roundf((vp.size.x - board_w) * 0.5)),
		maxf(0.0, roundf((vp.size.y - board_h) * 0.5)))), "平面按 1000×600 的棋盘居中（%s）" % str(plane.position))
	# 只有在**真窗口**下才断言"曲线在可见区域内"；headless 视口是 64×64 的假窗口。
	if DisplayServer.get_name() != "headless":
		var p0 := _board_to_canvas(line.get_point_position(0), line, plane)
		var p1 := _board_to_canvas(line.get_point_position(total - 1), line, plane)
		ok(vp.has_point(p0) and vp.has_point(p1),
			"整条曲线落在可见视口内（首 %s / 末 %s）" % [str(p0), str(p1)])

	# 9) 场景报告（贴数字用）
	var rep: Dictionary = scene.call("report")
	print("      report: fed=%d line=%d draws=%d rects=%d writes=%d processes=%d reveal_redraws=%d"
		% [rep.get("fed_points", -1), rep.get("line_points", -1),
			rep.get("plane_draw_calls", -1), rep.get("plane_rect_total", -1),
			rep.get("function_write_total", -1), rep.get("plane_process_calls", -1),
			rep.get("reveal_redraw_count", -1)])
	ok(int(rep.get("fed_points", -1)) == total and int(rep.get("line_points", -1)) == total,
		"report()：fed_points == line_points == %d" % total)
	ok(int(rep.get("plane_draw_calls", 0)) >= 1, "report()：_draw 次数 >= 1")

	scene.queue_free()
	await process_frame
	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("PLANE_ON_SCREEN %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
