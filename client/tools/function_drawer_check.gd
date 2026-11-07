extends SceneTree
## `FunctionDrawer` 真节点验收脚本（`docs/16-client.md` §40）。
##
## ```text
## cargo build --features gdext-bindings
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## godot --headless --path client --import
## godot --headless --path client --script res://tools/function_drawer_check.gd
## ```
##
## 它验证的是「**复原的增量追加语义能不能真的落到 `Line2D` 上**」，不验证复原逻辑本身：
##
## 1. `FunctionDrawer` 注册进了 `ClassDB`、是 `Control` 子类、能 `instantiate`；
## 2. **增量**：喂 10 点 ⇒ 10 点；再喂 **12 个"前 10 个都不一样"**的点 ⇒ 只加到 12 点，
##    且 `get_point_position(0..9)` **仍是第一次喂的值**（这是"没重画"的**引擎侧**证据，
##    不是只看计数）；
## 3. **变短不缩**：喂 5 点 ⇒ 点数仍 12、`add_point` 次数仍 12、已写入数仍 12；
## 4. 每个 player id 一条 `Line2D`，互不干扰；`remove_function` 之后重建是**新线**（从 0 点开始）；
## 5. `update_function_fade` 改的是 value 里那份 `Color` 的 alpha，再写回 `Line2D`；
## 6. `update_transform` 的 4 个被调者：位置 / 宽度 **0.125** / 缩放（`flip` 取负）/ 接点 **2**；
## 7. `GraphwarPlane::build_children` 建出来的 3 个子节点**就是 `FunctionDrawer`**
##    （不再是 `@Control@N`），进树之后 `ready` 不会重建。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const FD := "FunctionDrawer"
const PLANE := "GraphwarPlane"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## `update_function` 的 color 实参（原版 `r9d`，`0x00RRGGBB`：低字节 = R）。
## 取 `0x11_22_33` ⇒ `Color(0x33/255, 0x22/255, 0x11/255, 1)`，三个分量互不相同，抄错字节顺序会立刻红。
const PACKED_COLOR := 0x11_22_33
const EXPECTED_COLOR := Color(0x33 / 255.0, 0x22 / 255.0, 0x11 / 255.0, 1.0)

## 原版 `update_transform` 的两个常量（`0x1baff8` / `esi = 2`）。
const LINE_WIDTH := 0.125
const LINE_JOINT := 2

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

## 通用：类注册 + 实例化 + 父类 + 方法表。
func _check_class(cls: String, wanted: Array) -> Object:
	ok(ClassDB.class_exists(cls), "ClassDB.class_exists(\"%s\")" % cls)
	if not ClassDB.class_exists(cls):
		return null
	var inst: Object = ClassDB.instantiate(cls)
	ok(inst != null and inst is Control, "%s 能实例化且是 Control" % cls)
	if inst == null:
		return null
	ok(inst.is_class("Control"), "%s 的 is_class(\"Control\") 为真（父类正确）" % cls)
	print("      %s.get_class() = %s" % [cls, inst.get_class()])
	var names := _method_names(inst)
	for w in wanted:
		ok(names.has(w), "%s 的方法 %s() 存在" % [cls, w])
	return inst

## 造 n 个点：`base + i`（x）与 `-base - 2i`（y）。两批 `base` 不同 ⇒ 前 10 个点一定不同，
## 这样"没重画"才能用 `get_point_position` 证明。
func _make_points(base: float, n: int) -> PackedVector2Array:
	var out := PackedVector2Array()
	for i in range(n):
		out.append(Vector2(base + float(i), -base - 2.0 * float(i)))
	return out

## 从 drawer 的子节点里按 `line_instance_id()` 找出那条 `Line2D`。
func _line_by_id(drawer: Control, player_id: int) -> Line2D:
	var want: int = int(drawer.call("line_instance_id", player_id))
	if want == -1:
		return null
	for i in range(drawer.get_child_count()):
		var kid: Node = drawer.get_child(i)
		if kid is Line2D and kid.get_instance_id() == want:
			return kid
	return null

# ---------------------------------------------------------------------------
# 1. 增量追加：10 → 12 → 5（本脚本的核心）
# ---------------------------------------------------------------------------

func _check_incremental(drawer: Object) -> void:
	print("--- 增量追加（10 → 12 → 5）")
	var c: Control = drawer

	ok(int(drawer.call("function_count")) == 0, "刚建出来 function_count() == 0")
	ok(c.get_child_count() == 0, "刚建出来没有子节点")
	ok(int(drawer.call("point_count", 1)) == -1, "没有的 id：point_count(1) == -1")
	ok(int(drawer.call("written", 1)) == -1, "没有的 id：written(1) == -1")
	ok(is_equal_approx(float(drawer.call("stored_alpha", 1)), -1.0),
		"没有的 id：stored_alpha(1) == -1.0")

	# ---- 第一次：10 个点 ----
	var a := _make_points(0.0, 10)
	print("      第一次 10 点：%s … %s" % [a[0], a[9]])
	ok(int(drawer.call("update_function", 1, a, PACKED_COLOR)) == 10,
		"第一次喂 10 点 ⇒ 返回 10（追加了 10 个）")
	ok(int(drawer.call("function_count")) == 1, "function_count() == 1")
	ok(c.get_child_count() == 1, "子节点个数 == 1（每个 player id 一条 Line2D）")
	ok(int(drawer.call("created_line_total")) == 1, "created_line_total() == 1")
	var line := _line_by_id(c, 1)
	ok(line != null, "那条子节点是 Line2D（line_instance_id 能对到它）")
	if line == null:
		return
	ok(String(line.get_class()) == "Line2D", "子节点的 get_class() == \"Line2D\"")
	ok(line.get_point_count() == 10, "Line2D.get_point_count() == 10（实际 %d）"
		% line.get_point_count())
	ok(int(drawer.call("add_point_total")) == 10,
		"add_point 调用次数 == 10（实际 %d）" % int(drawer.call("add_point_total")))
	ok(int(drawer.call("written", 1)) == 10, "value 里的已写入数 == 10")
	ok(int(drawer.call("point_count", 1)) == int(drawer.call("written", 1)),
		"点数 == 已写入数（第一次）")
	ok(line.get_point_position(0).is_equal_approx(a[0]),
		"get_point_position(0) == 喂进去的第 0 点")
	ok(line.get_point_position(9).is_equal_approx(a[9]),
		"get_point_position(9) == 喂进去的第 9 点")
	ok(line.get_default_color().is_equal_approx(EXPECTED_COLOR),
		"建节点时 set_default_color(低字节=R)：%s" % str(line.get_default_color()))

	# ---- 第二次：12 个点，**前 10 个与第一批全不一样** ----
	var b := _make_points(100.0, 12)
	ok(not a[0].is_equal_approx(b[0]) and not a[9].is_equal_approx(b[9]),
		"对照：第二批的前 10 个点与第一批确实不同（否则下面'没重画'的断言不成立）")
	ok(int(drawer.call("update_function", 1, b, PACKED_COLOR)) == 2,
		"再喂 12 点 ⇒ **只追加 2 个**（返回 2，不是 12）")
	ok(line.get_point_count() == 12, "点数 == 12（实际 %d）" % line.get_point_count())
	ok(int(drawer.call("add_point_total")) == 12,
		"add_point 调用次数 == 12（= 10 + 2，不是 10 + 12）")
	ok(int(drawer.call("written", 1)) == 12, "已写入数推进到 12")
	ok(line.get_point_position(0).is_equal_approx(a[0]),
		"⚠️ 第 0 点仍是**第一批**的值 ⇒ 没有重画整条曲线")
	ok(line.get_point_position(9).is_equal_approx(a[9]),
		"第 9 点仍是第一批的值")
	ok(line.get_point_position(10).is_equal_approx(b[10]),
		"第 10 点是第二批的新点")
	ok(line.get_point_position(11).is_equal_approx(b[11]),
		"第 11 点是第二批的新点")

	# ---- 第三次：5 个点（变短）----
	var shorter := _make_points(-50.0, 5)
	var appended: int = int(drawer.call("update_function", 1, shorter, PACKED_COLOR))
	ok(appended == 0, "喂 5 点（变短）⇒ 返回 0（一个点都没加，实际 %d）" % appended)
	ok(line.get_point_count() == 12,
		"点数仍 == 12（曲线不会因为函数变短而缩短，实际 %d）" % line.get_point_count())
	ok(int(drawer.call("add_point_total")) == 12,
		"add_point 调用次数仍 == 12（实际 %d）—— 这条区分'增量'与'重画'"
			% int(drawer.call("add_point_total")))
	ok(int(drawer.call("written", 1)) == 12,
		"已写入数仍是 12（原版提前 return 的那条不写回）")
	ok(line.get_point_position(0).is_equal_approx(a[0]), "点还是老的（第一点没动）")
	ok(c.get_child_count() == 1, "变短没有建新节点（还是 1 条线）")
	ok(int(drawer.call("rejected_call_total")) == 0, "正常调用没有被计入 rejected")

# ---------------------------------------------------------------------------
# 2. 多个 player id 各自一条线
# ---------------------------------------------------------------------------

func _check_players(drawer: Object) -> void:
	print("--- 多玩家")
	var c: Control = drawer
	var p2 := _make_points(7.0, 4)
	ok(int(drawer.call("update_function", 2, p2, 0xff_00_00)) == 4,
		"player 2 喂 4 点 ⇒ 返回 4")
	ok(int(drawer.call("function_count")) == 2, "function_count() == 2")
	ok(c.get_child_count() == 2, "子节点 == 2")
	ok(int(drawer.call("point_count", 1)) == 12, "player 1 仍是 12 点（互不干扰）")
	ok(int(drawer.call("point_count", 2)) == 4, "player 2 是 4 点")
	ok(int(drawer.call("written", 2)) == 4, "player 2 的已写入数 == 4")
	ok(int(drawer.call("add_point_total")) == 16, "add_point 总数 == 16（12 + 4）")
	var l2 := _line_by_id(c, 2)
	ok(l2 != null, "player 2 有自己的 Line2D")
	ok(int(drawer.call("line_instance_id", 1)) != int(drawer.call("line_instance_id", 2)),
		"两条线的 instance id 不同")
	if l2 != null:
		# 原版的打包规则是"**低字节 = R**"（`0x498d74`–`0x498da1`）⇒ `0xff0000` 其实是**蓝**
		# （r = 0x00、g = 0x00、b = 0xff）。这条同时把字节序钉死：抄成 `0xRRGGBB` 的直觉会红。
		ok(l2.get_default_color().is_equal_approx(Color(0.0, 0.0, 1.0, 1.0)),
			"player 2 的颜色 0xff0000 按'低字节=R'解出来是**蓝**：%s"
				% str(l2.get_default_color()))

# ---------------------------------------------------------------------------
# 3. remove_function + 重建
# ---------------------------------------------------------------------------

func _check_remove(drawer: Object) -> void:
	print("--- remove_function + 重建")
	var c: Control = drawer
	var old_id: int = int(drawer.call("line_instance_id", 1))
	ok(bool(drawer.call("remove_function", 1)) == true, "remove_function(1) == true")
	ok(int(drawer.call("function_count")) == 1, "function_count() == 1")
	ok(int(drawer.call("removed_line_total")) == 1, "removed_line_total() == 1")
	ok(int(drawer.call("line_instance_id", 1)) == -1, "表里已经没有 player 1")
	ok(int(drawer.call("point_count", 1)) == -1, "player 1 的 point_count() == -1")
	ok(bool(drawer.call("remove_function", 1)) == false, "再删一次 == false")
	ok(int(drawer.call("rejected_call_total")) == 1, "无此 id 的 remove 计入 rejected（1）")

	# `queue_free` 要到帧末才回收 ⇒ 等一帧再数子节点。
	await process_frame
	ok(c.get_child_count() == 1, "queue_free 生效后子节点 == 1（实际 %d）" % c.get_child_count())

	# 重建：**新线、从 0 点开始**（原版 `remove_function` 是 queue_free，重建不复用旧点）。
	var fresh := _make_points(200.0, 10)
	ok(int(drawer.call("update_function", 1, fresh, PACKED_COLOR)) == 10,
		"重建 player 1：喂 10 点 ⇒ 返回 10（新线从 0 开始，不是 0 增量）")
	await process_frame
	ok(c.get_child_count() == 2, "重建后子节点 == 2（实际 %d）" % c.get_child_count())
	ok(int(drawer.call("created_line_total")) == 3, "created_line_total() == 3")
	var line := _line_by_id(c, 1)
	ok(line != null and line.get_instance_id() != old_id, "重建出来的是一条**新** Line2D")
	if line != null:
		ok(line.get_point_count() == 10, "新线的点数 == 10")
		ok(line.get_point_position(0).is_equal_approx(fresh[0]), "新线的第 0 点是新喂的值")

# ---------------------------------------------------------------------------
# 4. update_function_fade：改 value 里那份 Color 的 alpha
# ---------------------------------------------------------------------------

func _check_fade(drawer: Object) -> void:
	print("--- update_function_fade")
	var line := _line_by_id(drawer, 1)
	ok(is_equal_approx(float(drawer.call("update_function_fade", 1, 0.25)), 0.25),
		"update_function_fade(1, 0.25) 返回 0.25")
	ok(is_equal_approx(float(drawer.call("stored_alpha", 1)), 0.25),
		"value 里 Color 的 alpha == 0.25（原版先改 value、再 set_default_color）")
	ok(int(drawer.call("fade_call_total")) == 1, "fade_call_total() == 1")
	if line != null:
		ok(is_equal_approx(line.get_default_color().a, 0.25),
			"Line2D 的 default_color.a == 0.25")
		ok(line.get_default_color().is_equal_approx(
				Color(EXPECTED_COLOR.r, EXPECTED_COLOR.g, EXPECTED_COLOR.b, 0.25)),
			"只改 alpha，r/g/b 仍是建节点时那三个字节")
	var other := _line_by_id(drawer, 2)
	if other != null:
		ok(is_equal_approx(other.get_default_color().a, 1.0),
			"player 2 的 alpha 不受影响（还是 1.0）")
	ok(is_equal_approx(float(drawer.call("update_function_fade", 99, 0.5)), -1.0),
		"无此 id ⇒ 返回 -1.0")
	ok(int(drawer.call("rejected_call_total")) == 2, "无此 id 的 fade 计入 rejected（2）")

# ---------------------------------------------------------------------------
# 5. update_transform：位置 / 宽度 / 缩放 / 接点
# ---------------------------------------------------------------------------

func _check_transform(drawer: Object) -> void:
	print("--- update_transform")
	var c: Control = drawer
	ok(int(drawer.call("update_transform", 2.0, 3.0, Vector2(1.5, 0.5), false)) == 2,
		"update_transform(2,3,Vector2(1.5,0.5),false) 动了 2 条线")
	ok(int(drawer.call("transform_line_total")) == 2, "transform_line_total() == 2")
	var line := _line_by_id(c, 1)
	var other := _line_by_id(c, 2)
	if line != null:
		ok(line.position.is_equal_approx(Vector2(2.0, 3.0)),
			"每条线 set_position(Vector2(2,3))：%s" % str(line.position))
		ok(is_equal_approx(line.width, LINE_WIDTH),
			"set_width(0.125)：实际 %f" % line.width)
		ok(line.scale.is_equal_approx(Vector2(1.5, 0.5)),
			"flip=false ⇒ set_scale(原样)：%s" % str(line.scale))
		ok(line.joint_mode == LINE_JOINT, "set_joint_mode(2)：实际 %d" % line.joint_mode)
	if other != null:
		ok(is_equal_approx(other.width, LINE_WIDTH), "player 2 的线也被设了宽度 0.125")
	ok(int(drawer.call("update_transform", 0.0, 0.0, Vector2(2.0, 2.0), true)) == 2,
		"flip=true 也动了 2 条线")
	ok(int(drawer.call("transform_line_total")) == 4, "transform_line_total() 累加到 4")
	if line != null:
		ok(line.scale.is_equal_approx(Vector2(-2.0, -2.0)),
			"flip=true ⇒ 0x19e880 的四个 -0.0 取负：%s" % str(line.scale))
		ok(line.position.is_equal_approx(Vector2.ZERO), "位置改成 (0,0)")
		ok(line.get_point_count() == 10, "update_transform 不动点（还是 10 个）")

# ---------------------------------------------------------------------------
# 6. 边界：空数组会建节点；超 i32 的 id 被拒
# ---------------------------------------------------------------------------

func _check_edges(drawer: Object) -> void:
	print("--- 边界")
	var c: Control = drawer
	ok(int(drawer.call("update_function", 7, PackedVector2Array(), PACKED_COLOR)) == 0,
		"喂空数组 ⇒ 返回 0（一个点都没加）")
	ok(int(drawer.call("function_count")) == 3,
		"但节点已经建了（原版建节点在长度判定之前）⇒ function_count() == 3")
	ok(int(drawer.call("point_count", 7)) == 0, "那条线 0 个点")
	ok(int(drawer.call("written", 7)) == 0, "那条线的已写入数 0")
	ok(bool(drawer.call("remove_function", 7)) == true, "把空线删掉")

	var huge := 1 << 40
	var before: int = int(drawer.call("rejected_call_total"))
	ok(int(drawer.call("update_function", huge, _make_points(0.0, 3), PACKED_COLOR)) == -1,
		"player_id 超出 i32 ⇒ update_function 返回 -1")
	ok(is_equal_approx(float(drawer.call("update_function_fade", huge, 0.5)), -1.0),
		"player_id 超出 i32 ⇒ update_function_fade 返回 -1.0")
	ok(bool(drawer.call("remove_function", huge)) == false,
		"player_id 超出 i32 ⇒ remove_function 返回 false")
	ok(int(drawer.call("rejected_call_total")) == before + 3, "三次越界都被计入 rejected")
	await process_frame
	ok(c.get_child_count() == 2, "删掉空线、拒绝越界之后子节点仍是 2（实际 %d）"
		% c.get_child_count())

# ---------------------------------------------------------------------------
# 7. GraphwarPlane 的 3 个子节点**就是** FunctionDrawer
# ---------------------------------------------------------------------------

func _check_plane() -> void:
	print("--- %s" % PLANE)
	var plane: Object = _check_class(PLANE, ["build_children", "function_drawer_count"])
	if plane == null:
		return
	var c: Control = plane
	c.set_size(Vector2(1000.0, 600.0))

	ok(int(plane.call("build_children")) == 3, "build_children() 返回 3")
	ok(c.get_child_count() == 3, "子节点个数 == 3（实际 %d）" % c.get_child_count())
	var names := PackedStringArray()
	var fd_count := 0
	var ctrl_count := 0
	for i in range(c.get_child_count()):
		var kid: Node = c.get_child(i)
		names.append("%s(%s)" % [kid.get_class(), kid.name])
		if kid is FunctionDrawer:
			fd_count += 1
		if kid is Control:
			ctrl_count += 1
	print("      子节点: %s" % ", ".join(names))
	ok(fd_count == 3, "3 个子节点全是 FunctionDrawer（实际 %d）" % fd_count)
	ok(ctrl_count == 3, "它们仍然是 Control 子类（实际 %d）" % ctrl_count)
	ok(String(c.get_child(0).name).contains("FunctionDrawer"),
		"自动名里带类名：%s（不再是 @Control@N）" % String(c.get_child(0).name))

	# 真的能喂：只喂第 1 个 drawer，另外两个必须还是 0 条线。
	var pts := _make_points(0.0, 6)
	ok(int(c.get_child(0).call("update_function", 5, pts, 0x00_00_ff)) == 6,
		"plane 的第 0 个子节点能 update_function（追加 6）")
	ok(int(c.get_child(0).call("point_count", 5)) == 6, "它的 player 5 有 6 个点")
	ok(int(c.get_child(1).call("function_count")) == 0, "第 1 个子节点没有曲线")
	ok(int(c.get_child(2).call("function_count")) == 0, "第 2 个子节点没有曲线")
	ok(int(plane.call("function_drawer_count")) == 3, "plane.function_drawer_count() == 3")

	root.add_child(c)
	await process_frame
	await process_frame
	ok(int(plane.call("ready_call_count")) >= 1, "进树之后 _ready 被引擎调到")
	ok(c.get_child_count() == 3, "`ready` 没有重复建子节点（还是 3 个）")
	var still_fd := 0
	for i in range(c.get_child_count()):
		if c.get_child(i) is FunctionDrawer:
			still_fd += 1
	ok(still_fd == 3, "进树之后 3 个还是 FunctionDrawer（实际 %d）" % still_fd)
	ok(int(c.get_child(0).call("point_count", 5)) == 6,
		"进树/`_ready` 不会清掉已经喂进去的点（还是 6）")

	c.queue_free()
	await process_frame

# ---------------------------------------------------------------------------
# 入口
# ---------------------------------------------------------------------------

func _initialize() -> void:
	# 0) 扩展加载了吗？第一次跑到（没有 `.godot/extension_list.cfg`）时不会，显式补一次。
	if not ClassDB.class_exists(FD):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	ok(ClassDB.class_exists("GW2Rebuild"), "对照组：单例 GW2Rebuild 仍在")
	ok(ClassDB.class_exists("AxisDrawer"), "对照组：AxisDrawer 仍在（§31）")
	ok(ClassDB.class_exists("EntityDrawer"), "对照组：EntityDrawer 仍在（§34）")
	ok(ClassDB.class_exists("EffectDrawer"), "对照组：EffectDrawer 仍在（§37）")

	var drawer: Object = _check_class(FD, [
		"update_function", "remove_function", "update_transform", "update_function_fade",
		"function_count", "point_count", "written", "stored_alpha", "line_instance_id",
		"add_point_total", "created_line_total", "removed_line_total",
		"rejected_call_total", "transform_line_total", "fade_call_total",
	])
	if drawer != null:
		# 有界否定：原版 `FunctionDrawer::draw` 在 x86 侧没有落点 ⇒ 本类**没有** `_draw`。
		var has_draw := false
		for m in drawer.get_method_list():
			if String(m.name) == "draw_call_count":
				has_draw = true
		ok(not has_draw, "有界否定：本类不暴露 `draw` 计数（x86 侧 FunctionDrawer::draw 未定位）")
		await _check_incremental(drawer)
		await _check_players(drawer)
		await _check_remove(drawer)
		await _check_fade(drawer)
		await _check_transform(drawer)
		await _check_edges(drawer)
		drawer.queue_free()
		await process_frame

	# GDScript 的 `.new()`：扩展类的第三条实例化路径。
	var fresh: Object = FunctionDrawer.new()
	ok(fresh != null and fresh is Control, "FunctionDrawer.new() 出来的是 Control")
	if fresh != null:
		var fw: Control = fresh
		ok(int(fw.call("function_count")) == 0, "新实例是空的")
		fw.free()

	await _check_plane()
	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("FUNCTION_DRAWER %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
