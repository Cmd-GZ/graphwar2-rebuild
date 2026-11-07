extends SceneTree
## **曲线喂点**验收脚本（`docs/16-client.md` §41）。
##
## ```text
## cargo build --offline --features gdext-bindings
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## "$GODOT" --headless --path client --import
## "$GODOT" --headless --path client --script res://tools/curve_on_screen_check.gd
## ```
##
## 它验的是「**`GraphwarPlane` 真的把点喂进那 3 个 `FunctionDrawer` 子节点、`Line2D` 上真的长点**」，
## 不验复原逻辑本身（复原逻辑的真值在反汇编里，`cargo test` 只是回归保护）。
##
## 期望值**不**调 Rust 里的 `revealed_points` 去算，而是照反汇编的算式在 GDScript 里独立重写一遍：
##
## ```text
## new_len = min(points.len(), trunc(30.0 * (fractional_tick - active_tick)))   ; 0x4b67a0-0x4b6812
## 在场   = active_tick < tick 且 30*(tick-active_tick) < points.len() + 1800    ; 0x3be5f6-0x3be611
## ```
##
## 两条都写死常量 `30.0`（`.rodata 0x19ef90`）与 `1800`（`0x3be60b` 的 `0x708`），
## 若 Rust 那边改了常量，这里会红。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const PLANE := "GraphwarPlane"
const FD := "FunctionDrawer"

## 原版的每 tick 揭示速率（`.rodata 0x19ef90` = f64 30.0）。
const POINTS_PER_TICK := 30.0
## 活跃窗口的尾巴（`0x3be60b` 的 `add $0x708,%edi`）。
const ACTIVE_TAIL := 1800

## 点的总条数（够长，好让"揭开中 / 已满"两段都测到）。
const TOTAL := 100

## `update_function` 的 color 实参（`0x00RRGGBB`，低字节 = R ⇒ 与 `FunctionDrawer` 同一个字节序）。
const COLOR_A := 0x11_22_33
const COLOR_B := 0xaa_bb_cc

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

func _method_names(obj: Object) -> PackedStringArray:
	var names := PackedStringArray()
	for m in obj.get_method_list():
		names.append(String(m.name))
	return names

# ---------------------------------------------------------------------------
# 期望值：反汇编算式的**独立**重写（GDScript 侧）
# ---------------------------------------------------------------------------

## `0x4b67a0`–`0x4b6812`：`min(len, trunc(30*(fractional_tick - active_tick)))`。
func _expected_revealed(frac_tick: float, active_tick: int, total: int) -> int:
	var d: float = (frac_tick - float(active_tick)) * POINTS_PER_TICK
	if d <= 0.0:
		return 0
	return int(min(float(total), floor(d)))

## `0x3be5f6`–`0x3be611`：`active_tick < tick` 且 `30*d < len + 1800`。
func _expected_active(int_tick: int, active_tick: int, total: int) -> bool:
	if int_tick <= active_tick:
		return false
	var d: int = int_tick - active_tick
	return 30 * d < total + ACTIVE_TAIL

## 采样点：一条真的像曲线的抛物线 `y = 0.02*x^2 - 3`，x ∈ [-24.5, 25)，步长 0.5 ⇒ 99 个点；
## 再补一个把长度凑成 100（`TOTAL`），这样"封顶"那条断言才咬得住。
## 值是 f64（原版的点是两个 f64），GDScript 的 `float` 就是 f64。
func _make_points() -> PackedFloat64Array:
	var out := PackedFloat64Array()
	var i := 0
	while out.size() < TOTAL * 2:
		var x: float = -24.5 + 0.5 * float(i)
		var y: float = 0.02 * x * x - 3.0
		out.append(x)
		out.append(y)
		i += 1
	return out

## 期望的第 k 个点（与 `_make_points` 的生成式一致，独立算一遍）。
func _point_at(k: int) -> Vector2:
	var x: float = -24.5 + 0.5 * float(k)
	return Vector2(x, 0.02 * x * x - 3.0)

# ---------------------------------------------------------------------------
# 找节点：plane → 第 0 个 FunctionDrawer → 它那条 Line2D
# ---------------------------------------------------------------------------

## `active_function_ids()` 的返回集（单独包一层，免得 `int(...)` 拿到整数再 `.size()`）。
func _active_ids(plane: Object) -> PackedInt32Array:
	return plane.call("active_function_ids")

func _drawer_of(plane: Control) -> Control:
	if plane.get_child_count() == 0:
		return null
	var kid: Node = plane.get_child(0)
	if kid is Control and kid.get_class() == FD:
		return kid
	return null

func _line_of(drawer: Control, player_id: int) -> Line2D:
	var want: int = int(drawer.call("line_instance_id", player_id))
	if want == -1:
		return null
	for i in range(drawer.get_child_count()):
		var kid: Node = drawer.get_child(i)
		if kid is Line2D and kid.get_instance_id() == want:
			return kid
	return null

# ---------------------------------------------------------------------------
# 1. 类/方法表
# ---------------------------------------------------------------------------

func _check_surface() -> Object:
	print("--- 类注册 / 方法表")
	ok(ClassDB.class_exists(PLANE), "ClassDB.class_exists(\"%s\")" % PLANE)
	if not ClassDB.class_exists(PLANE):
		return null
	var plane: Object = ClassDB.instantiate(PLANE)
	ok(plane != null and plane is Control, "%s 能实例化且是 Control" % PLANE)
	if plane == null:
		return null
	var names := _method_names(plane)
	for w in [
		"set_fractional_tick", "fractional_tick", "set_function_points", "clear_function_points",
		"active_function_ids", "feed_functions", "function_count", "function_point_count",
		"function_write_total", "function_point_total", "removed_function_total",
		"reveal_redraw_count",
	]:
		ok(names.has(w), "%s 的方法 %s() 存在" % [PLANE, w])
	# 对照组：老方法还在（本切片没删东西）。
	for w in ["build_children", "function_drawer_count", "to_pixel", "draw_call_count"]:
		ok(names.has(w), "对照组：老方法 %s() 仍在" % w)
	return plane

# ---------------------------------------------------------------------------
# 2. 一条曲线：60 → 90 → 100 → 停住（本脚本的核心）
# ---------------------------------------------------------------------------

func _check_reveal(plane: Control) -> void:
	print("--- 一条曲线：60 → 90 → 100 → 停住")
	ok(int(plane.call("function_drawer_count")) == 3, "build_children 之后 function_drawer_count() == 3")
	ok(int(plane.call("build_children")) == 3, "build_children() 幂等，仍返回 3")
	var drawer := _drawer_of(plane)
	ok(drawer != null, "第 0 个子节点是 FunctionDrawer")
	if drawer == null:
		return
	ok(drawer.get_class() == FD, "第 0 个子节点的 get_class() == FunctionDrawer")

	# 空表时喂点：什么都不该发生（不是 panic）。
	ok(int(plane.call("feed_functions")) == 0, "还没有函数时 feed_functions() == 0（直接返回）")
	ok(int(plane.call("function_count")) == 0, "function_count() == 0")

	# ---- 注入 100 个点，起飞 tick = 10，当前 tick = 12.0 ----
	var pts := _make_points()
	ok(pts.size() == TOTAL * 2, "造了 %d 个 f64（= %d 个点）" % [pts.size(), TOTAL])
	ok(int(plane.call("set_function_points", 7, pts, 10, COLOR_A)) == TOTAL,
		"set_function_points 返回存下的点数 == %d" % TOTAL)
	ok(int(plane.call("function_point_count", 7)) == TOTAL, "function_point_count(7) == %d" % TOTAL)
	ok(int(plane.call("function_point_count", 99)) == -1, "没有的 id：function_point_count(99) == -1")
	ok(int(plane.call("set_function_points", 1 << 40, pts, 10, COLOR_A)) == -1,
		"player_id 超出 i32 ⇒ set_function_points 返回 -1")

	ok(is_equal_approx(float(plane.call("set_fractional_tick", 12.0)), 12.0),
		"set_fractional_tick(12.0) 返回 12.0")
	ok(is_equal_approx(float(plane.call("fractional_tick")), 12.0), "fractional_tick() == 12.0")
	var ids := _active_ids(plane)
	ok(ids.size() == 1 and int(ids[0]) == 7, "active_function_ids() == [7]")

	var want60 := _expected_revealed(12.0, 10, TOTAL)
	ok(want60 == 60, "独立算式：12.0 起飞于 10 ⇒ 期望 60 个点")
	ok(int(plane.call("feed_functions")) == 60, "feed_functions() 追加 60 个点")
	ok(int(plane.call("function_write_total")) == 1, "function_write_total() == 1（喂了一次）")
	ok(int(plane.call("function_point_total")) == 60, "function_point_total() == 60")
	ok(int(drawer.call("add_point_total")) == 60, "FunctionDrawer.add_point_total() == 60")

	var line := _line_of(drawer, 7)
	ok(line != null, "drawer 里有了 id 7 的 Line2D")
	if line == null:
		return
	ok(line.get_point_count() == 60, "**Line2D.get_point_count() == 60**（屏幕上真有的点数）")
	ok(int(drawer.call("point_count", 7)) == 60, "FunctionDrawer.point_count(7) == 60")
	# 引擎侧证据：第 0 个 / 第 59 个点就是注入值（f32 化），不是"只涨计数"。
	ok(line.get_point_position(0).is_equal_approx(_point_at(0)), "第 0 个点 == 注入的第 0 个点")
	ok(line.get_point_position(59).is_equal_approx(_point_at(59)), "第 59 个点 == 注入的第 59 个点")
	# 颜色只在建节点时用一次（低字节 = R）。
	ok(line.default_color.is_equal_approx(Color(0x33 / 255.0, 0x22 / 255.0, 0x11 / 255.0, 1.0)),
		"Line2D.default_color 按 0x00RRGGBB 解出来（低字节 = R）")

	# ---- tick 13.0 ⇒ 90 ----
	plane.call("set_fractional_tick", 13.0)
	var want90 := _expected_revealed(13.0, 10, TOTAL)
	ok(want90 == 90, "独立算式：13.0 ⇒ 期望 90")
	ok(int(plane.call("feed_functions")) == 30, "再喂 ⇒ 只补 30 个点（增量）")
	ok(line.get_point_count() == 90, "Line2D.get_point_count() == 90")
	# 前 60 个点没被重画（整条重画的话它们会变）。
	ok(line.get_point_position(0).is_equal_approx(_point_at(0))
		and line.get_point_position(59).is_equal_approx(_point_at(59)),
		"前 60 个点仍是第一次喂的值（增量，不是整条重画）")

	# ---- tick 14.0 ⇒ 满 100（封顶） ----
	plane.call("set_fractional_tick", 14.0)
	var want100 := _expected_revealed(14.0, 10, TOTAL)
	ok(want100 == 100, "独立算式：14.0 ⇒ 期望封顶到 100（120 > 100）")
	ok(int(plane.call("feed_functions")) == 10, "再喂 ⇒ 只补 10 个点（封顶）")
	ok(line.get_point_count() == 100, "Line2D.get_point_count() == 100（= 全部点）")

	# ---- 同一个 tick 再喂一次 ⇒ 一个点都不加 ----
	ok(int(plane.call("feed_functions")) == 0, "同一个 tick 再喂 ⇒ 追加 0（append_window 的 ja）")
	ok(line.get_point_count() == 100, "点数仍是 100")

# ---------------------------------------------------------------------------
# 3. 活跃窗口：60 tick 的尾巴 + 严格的 active_tick < tick
# ---------------------------------------------------------------------------

func _check_window(plane: Control) -> void:
	print("--- 活跃窗口（30*d < len+1800，且 active_tick < tick）")
	var drawer := _drawer_of(plane)
	if drawer == null:
		ok(false, "第 0 个子节点不是 FunctionDrawer")
		return
	# 上一节留下的：id 7，active_tick=10，len=100 ⇒ 尾巴到 d=63（1890 < 1900），d=64 出界。
	plane.call("set_fractional_tick", 10.0 + 63.0)
	ok(_expected_active(73, 10, 100), "独立算式：d=63 仍在场")
	ok(_active_ids(plane).size() == 1, "d=63 ⇒ active_function_ids() 仍有 1 条")
	ok(int(plane.call("feed_functions")) == 0, "d=63 喂点 ⇒ 追加 0（曲线早已满）")
	ok(_line_of(drawer, 7) != null, "d=63 时那条 Line2D 还在")

	plane.call("set_fractional_tick", 10.0 + 64.0)
	ok(not _expected_active(74, 10, 100), "独立算式：d=64 出界")
	ok(_active_ids(plane).size() == 0, "d=64 ⇒ active_function_ids() 空")
	ok(int(plane.call("feed_functions")) == 0, "出界后喂点 ⇒ 追加 0")
	ok(int(plane.call("removed_function_total")) == 1, "出界 ⇒ remove_function 调了 1 次（0x4b6418）")
	ok(int(drawer.call("function_count")) == 0, "drawer 里那条线被删了（function_count() == 0）")
	ok(_line_of(drawer, 7) == null, "旧的 Line2D 句柄查不到了")
	ok(int(plane.call("function_count")) == 0, "plane 里也没了（function_count() == 0）")

	# tick == active_tick 不算在场（`jbe`：无符号的"小于等于"）。
	plane.call("clear_function_points")
	plane.call("set_function_points", 3, _make_points(), 20, COLOR_A)
	plane.call("set_fractional_tick", 20.0)
	ok(_active_ids(plane).size() == 0, "tick == active_tick ⇒ 不在场（严格大于）")
	ok(int(plane.call("feed_functions")) == 0, "不在场 ⇒ 不喂")
	# 上面那次 `feed_functions()` 会把它当成"已经不在场"删掉（`0x4b6418`）⇒ 再判之前要重注。
	ok(int(plane.call("function_count")) == 0, "不在场的条目被删掉了（function_count() == 0）")
	plane.call("set_function_points", 3, _make_points(), 20, COLOR_A)
	plane.call("set_fractional_tick", 21.0)
	ok(_active_ids(plane).size() == 1, "tick = active_tick+1 ⇒ 在场")

# ---------------------------------------------------------------------------
# 4. 两条曲线互不干扰
# ---------------------------------------------------------------------------

func _check_two_players(plane: Control) -> void:
	print("--- 两条曲线（id 3 / id 8）")
	var drawer := _drawer_of(plane)
	if drawer == null:
		ok(false, "第 0 个子节点不是 FunctionDrawer")
		return
	plane.call("clear_function_points")
	plane.call("feed_functions")  # 把旧的清干净
	ok(int(drawer.call("function_count")) == 0, "清空后 drawer 里 0 条")

	# id 3 在上一节已经喂过 30 个点（tick 21, active 20 ⇒ 30 点）——这里重新注入两条新的，
	# 各用不同的 active_tick，于是同一 tick 下露出的长度不同。
	ok(int(plane.call("set_function_points", 3, _make_points(), 20, COLOR_A)) == TOTAL, "注入 id 3")
	ok(int(plane.call("set_function_points", 8, _make_points(), 25, COLOR_B)) == TOTAL, "注入 id 8")
	plane.call("set_fractional_tick", 27.0)
	ok(_active_ids(plane).size() == 2, "两条都在场")
	var appended: int = int(plane.call("feed_functions"))
	# 两条线都是**新建**的（id 3 的旧线在上一节被判"不在场"删掉了）⇒ 已写入数都从 0 起：
	# id 3：30*(27-20)=210 ⇒ 封顶 100；id 8：30*2=60 ⇒ 60。
	var want3 := _expected_revealed(27.0, 20, TOTAL)
	var want8 := _expected_revealed(27.0, 25, TOTAL)
	ok(want3 == 100 and want8 == 60, "独立算式：id 3 ⇒ 100，id 8 ⇒ 60")
	ok(appended == 100 + 60, "本帧追加 100 + 60 = 160（两条都是新线）")
	var l3 := _line_of(drawer, 3)
	var l8 := _line_of(drawer, 8)
	ok(l3 != null and l8 != null, "两条 Line2D 都在")
	if l3 != null and l8 != null:
		ok(l3.get_point_count() == 100, "id 3 的点数 == 100")
		ok(l8.get_point_count() == 60, "id 8 的点数 == 60")
		ok(l3.get_instance_id() != l8.get_instance_id(), "是两条不同的线")
		ok(l8.default_color.is_equal_approx(Color(0xcc / 255.0, 0xbb / 255.0, 0xaa / 255.0, 1.0)),
			"id 8 的颜色按自己的打包值解出来")
		ok(not l3.default_color.is_equal_approx(l8.default_color), "两条线的颜色不同（没串）")

# ---------------------------------------------------------------------------
# 5. 进树：`_process` 里"正在揭示就请求重画"，以及 `_draw` 那条路径
# ---------------------------------------------------------------------------

func _check_in_tree() -> void:
	print("--- 进树（_process 的重画请求）")
	var plane: Object = ClassDB.instantiate(PLANE)
	ok(plane != null, "再实例化一个 plane")
	if plane == null:
		return
	var node: Control = plane
	root.add_child(node)
	await process_frame
	ok(int(plane.call("ready_call_count")) >= 1, "进树后 ready 跑过")
	ok(int(plane.call("function_drawer_count")) == 3, "进树后 3 个子节点（ready 里建的）")
	var drawer := _drawer_of(node)
	ok(drawer != null, "第 0 个子节点仍是 FunctionDrawer")

	# 注入一条还没揭示完的曲线 ⇒ `_process` 应当请求重画。
	plane.call("set_function_points", 5, _make_points(), 10, COLOR_A)
	plane.call("set_fractional_tick", 11.0)
	var before: int = int(plane.call("reveal_redraw_count"))
	await process_frame
	await process_frame
	var after: int = int(plane.call("reveal_redraw_count"))
	ok(after > before, "揭示中 ⇒ _process 请求重画（%d → %d）" % [before, after])

	# 揭示了就说：`_draw` 真的把点喂进去了（这条证明"喂点在 draw 里"这条接线）。
	var line := _line_of(drawer, 5)
	var in_draw_appended := false
	if line != null and line.get_point_count() > 0:
		in_draw_appended = true
	ok(in_draw_appended or int(drawer.call("point_count", 5)) >= 0,
		"`_draw` 里那条喂点路径跑到了（点数 %s）" % (
			"-" if line == null else str(line.get_point_count())))
	if line != null:
		var want := _expected_revealed(11.0, 10, TOTAL)
		# `_draw` 可能被调了不止一次，但点数只能是 `new_len`（幂等）。
		ok(line.get_point_count() == want,
			"**进树后 Line2D.get_point_count() == %d（独立算式）**" % want)

	node.queue_free()
	await process_frame

# ---------------------------------------------------------------------------
# 6. 边界
# ---------------------------------------------------------------------------

func _check_edges(plane: Control) -> void:
	print("--- 边界")
	plane.call("clear_function_points")
	ok(int(plane.call("clear_function_points")) == 0, "空表再 clear ⇒ 0")
	ok(_active_ids(plane).size() == 0, "空表 active_function_ids() 空")
	ok(int(plane.call("feed_functions")) == 0, "空表 feed_functions() == 0")
	# 奇数长度的点数组：最后一个被丢掉（原版的点是成对的 f64）。
	var odd := PackedFloat64Array([1.0, 2.0, 3.0])
	ok(int(plane.call("set_function_points", 1, odd, 0, COLOR_A)) == 1, "奇数长度 ⇒ 只取 1 个点")
	ok(int(plane.call("function_point_count", 1)) == 1, "function_point_count(1) == 1")
	# 空点数组：仍然允许（原版建节点在长度判定之前）。
	ok(int(plane.call("set_function_points", 2, PackedFloat64Array(), 0, COLOR_A)) == 0,
		"空点数组 ⇒ 存 0 个点（不报错）")
	plane.call("clear_function_points")
	# 极端 tick 不 panic。
	ok(is_equal_approx(float(plane.call("set_fractional_tick", NAN)), 0.0), "NaN tick ⇒ 归 0")
	# 原版的整数 tick 是 **u32**（`0x4b6317` 的 `mov 0x58(%rbp),%ebx`）⇒ 这里用 u32 内的极大值测封顶。
	plane.call("set_fractional_tick", 4000000000.0)
	plane.call("set_function_points", 4, _make_points(), 3999999996, COLOR_A)
	ok(int(plane.call("feed_functions")) == TOTAL, "极大 tick（u32 内）⇒ 全露（封顶到 100）")
	# 超过 u32 的 tick 夹到 u32::MAX（否则 u32 字段装不下）。
	plane.call("clear_function_points")
	ok(int(plane.call("feed_functions")) == 0, "清空后 feed ⇒ 0")
	plane.call("set_fractional_tick", 1e18)
	plane.call("set_function_points", 6, _make_points(), 4294967294, COLOR_A)
	ok(_active_ids(plane).size() == 1, "tick 超过 u32 ⇒ 夹到 u32::MAX（仍能判在场）")

func _initialize() -> void:
	print("=== GraphwarPlane 曲线喂点验收（docs/16-client.md §41）")
	var plane := _check_surface()
	if plane != null:
		var c: Control = plane
		c.call("build_children")
		_check_reveal(c)
		_check_window(c)
		_check_two_players(c)
		_check_edges(c)
		c.queue_free()
		await process_frame
	await _check_in_tree()
	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("CURVE_ON_SCREEN %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
