extends SceneTree
## 四个"只多一个 `_draw`"的类的真节点验收脚本（`docs/16-client.md` §40）。
##
## ```text
## cargo build --release --features gdext-bindings
## cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
## godot --headless --path client --import
## godot --headless --path client --script res://tools/draw_classes_check.gd
## ```
##
## 它验证的是「**复原的纯逻辑能不能真的变成引擎调用**」，不验证复原逻辑本身：
##
## 1. 四个类都注册进了 `ClassDB`、都是 `Control` 子类、能 `instantiate`；
## 2. `EffectDrawer`：7 个相位各发**恰好**一条图元（圆 / 多边形 / 线段），
##    多边形在"顶点未复原"时**不画**而是记 `polygon_skipped`；
## 3. `GraphwarLongPressAnimation`：`press()` 之后 `_process` 真的推进相位，
##    每次 `_draw` 发**两条** `draw_arc`，越过 2π 后自己写哨兵；
## 4. `GraphwarPlane`：建 **3 个**子节点、画 1 个棋盘框、脏矩形进出、`resized` 真的连上；
## 5. `PlayerShowcase`：读档案（名字 / 皮肤 / 脸 / 帽子 / 计数）、画三层贴图、
##    计数器变了才重画、**不读**段位；任务书要的 3 `Sprite2D` + 1 标签走 opt-in 的演示口。
##
## 与 `entity_drawer_check.gd`（§38）的分工：那个验"建子节点树"，这个验"发绘制调用"。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const FX := "EffectDrawer"
const LPA := "GraphwarLongPressAnimation"
const PLANE := "GraphwarPlane"
const SHOWCASE := "PlayerShowcase"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## `EffectDrawer` 的控件尺寸：`draw_transform` 用 `size * 0.5` 当原点，
## 所以只要不是 0 就行；这里与 §38 的验收脚本取同一个值，便于对照。
const FX_SIZE := Vector2(500.0, 300.0)

## 7 个相位的期望图元（见 `effect_drawer::Phase`）。
## `"circle"` = 发一条 `draw_circle`；`"line"` = 发一条 `draw_line`；
## `"polygon_skip"` = 顶点未复原 ⇒ 记一次 skipped、不调 `draw_polygon`。
const PHASE_KINDS := [
	"circle",       # 0 CircleSmall  r=0.05
	"polygon_skip", # 1 Polygon      顶点公式未复原
	"circle",       # 2 CircleLargeA r=0.06
	"circle",       # 3 CircleLargeB r=0.06
	"line",         # 4 LineA
	"line",         # 5 LineB
	"circle",       # 6 CircleDark    alpha 固定 0.7
]

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

# ---------------------------------------------------------------------------
# 1. EffectDrawer：7 个相位各发恰好一条图元
# ---------------------------------------------------------------------------

## 造一个进树、已 spawn 一个特效的 `EffectDrawer`，等两帧后返回它（`_draw` 已跑过）。
## ⚠️ 这里必须把 `start_tick` 放到"未来"（`set_now_tick(1e6)` 时 spawn，再把 tick 拨回 0）：
## `_process` 每帧会调 `step_effects`，而**相位 2 / 5 不是终态**（`STATE_TERMINAL_MASK = 0x5b`），
## 只要 `start_tick <= elapsed` 就会换相位（2→3、5→6）。本组要断言的是「**当前相位**画什么」，
## 不是相位推进（推进由 `step()` 那几条单独断言）。不这么做的话相位 5 第一帧就被换成 6 了
## —— 这正是本轮实测到的第一个坑。
func _fx_make(phase: int) -> Node:
	var inst: Object = ClassDB.instantiate(FX)
	var c: Control = inst
	c.set_size(FX_SIZE)
	inst.call("set_now_tick", 1000000)
	var made: int = int(inst.call("spawn_effect", 5.0, 5.0, phase, 600))
	ok(made == 1, "spawn_effect(5,5,%d,600) 返回 1（实际 %d）" % [phase, made])
	inst.call("set_now_tick", 0)
	root.add_child(c)
	await process_frame
	await process_frame
	return c

func _check_effect_drawer() -> void:
	print("--- %s" % FX)
	var drawer: Object = _check_class(FX, [
		"spawn_effect", "clear_effects", "effect_count", "now_tick", "set_now_tick",
		"step", "retain_alive", "effect_phase", "effect_alpha",
		"set_polygon_placeholder", "polygon_placeholder_count",
		"draw_call_count", "process_call_count", "ready_call_count",
		"drawn_transform_total", "drawn_circle_total", "drawn_polygon_total",
		"drawn_line_total", "polygon_skipped_total",
	])
	if drawer == null:
		return

	drawer.set("flip_y", false)
	ok(bool(drawer.get("flip_y")) == false, "属性 flip_y 往返一致（false）")

	var c: Control = drawer
	c.set_size(FX_SIZE)
	ok(c.size.is_equal_approx(FX_SIZE), "控件尺寸设为 %s" % FX_SIZE)

	# 元素 / tick / alpha 三条走纯逻辑。
	ok(int(drawer.call("spawn_effect", 5.0, 5.0, 0, 60)) == 1, "spawn_effect 返回 1")
	ok(int(drawer.call("effect_count")) == 1, "effect_count() == 1")
	ok(int(drawer.call("effect_phase", 0)) == 0, "effect_phase(0) == 0")
	ok(int(drawer.call("spawn_effect", 0.0, 0.0, 7, 60)) == -1, "相位 7 越界 ⇒ 返回 −1 且不建")
	ok(int(drawer.call("spawn_effect", 0.0, 0.0, -1, 60)) == -1, "相位 −1 越界 ⇒ 返回 −1")
	ok(int(drawer.call("effect_count")) == 1, "越界调用没有改变 effect_count()")
	ok(int(drawer.call("effect_phase", 99)) == -1, "越界下标 effect_phase(99) == −1")
	ok(is_equal_approx(float(drawer.call("effect_alpha", 0)), 0.9),
		"tick 0 的 alpha == 0.9（alpha_at 的快路）")
	drawer.call("set_now_tick", 30)
	ok(is_equal_approx(float(drawer.call("effect_alpha", 0)), 0.45),
		"tick 30 / 60 的 alpha == 0.45（(1−0.5)·0.9）")
	drawer.call("set_now_tick", 60)
	ok(is_equal_approx(float(drawer.call("effect_alpha", 0)), 0.0),
		"tick 60 的 alpha == 0（过期）")
	drawer.call("set_now_tick", 0)

	# `step` 走 `step_effects`：相位 2 → 3（其余相位是终态，不动）。
	drawer.call("clear_effects")
	drawer.call("set_now_tick", 0)
	drawer.call("spawn_effect", 0.0, 0.0, 2, 600)
	ok(int(drawer.call("step", 1.0)) == 60, "step(1.0) 推进 60 tick（60 tick = 1 秒）")
	ok(int(drawer.call("now_tick")) == 60, "now_tick() == 60")
	ok(int(drawer.call("effect_phase", 0)) == 3, "相位 2 到期后推进到 3（CircleLargeA → B）")
	ok(int(drawer.call("step", 0.5)) == 90, "再 step(0.5) ⇒ now_tick == 90")

	# `retain_alive`（推断判据 = `is_alive_at`）：tick 100 建、duration 10 ⇒ tick 111 就过期。
	drawer.call("clear_effects")
	drawer.call("set_now_tick", 100)
	drawer.call("spawn_effect", 0.0, 0.0, 0, 10)
	ok(int(drawer.call("retain_alive")) == 1, "tick 100 建、duration 10 ⇒ tick 100 还活着")
	drawer.call("set_now_tick", 111)
	ok(int(drawer.call("retain_alive")) == 0, "tick 111 > 100+10 ⇒ retain_alive() 丢掉它（剩 0）")

	# GDScript 的 `new()`：第三条实例化路径。
	var fresh: Object = EffectDrawer.new()
	ok(fresh != null and fresh is Control, "EffectDrawer.new() 出来的是 Control")
	if fresh != null:
		var fw: Control = fresh
		fw.free()

	# ---- 7 个相位，各自一个**独立实例**（这样计数可以直接比）----
	var n_circle := 0
	var n_line := 0
	var n_skip := 0
	for phase in range(PHASE_KINDS.size()):
		var node := await _fx_make(phase)
		var kind: String = PHASE_KINDS[phase]
		var draws: int = int(node.call("draw_call_count"))
		ok(draws >= 1, "相位 %d：_draw 被引擎调到（%d 次）" % [phase, draws])
		ok(int(node.call("drawn_transform_total")) == draws,
			"相位 %d：每次 _draw 发 1 条 draw_set_transform（%d/%d）"
				% [phase, int(node.call("drawn_transform_total")), draws])
		var circles: int = int(node.call("drawn_circle_total"))
		var lines: int = int(node.call("drawn_line_total"))
		var polys: int = int(node.call("drawn_polygon_total"))
		var skips: int = int(node.call("polygon_skipped_total"))
		if kind == "circle":
			n_circle += 1
			ok(circles == draws, "相位 %d：每次 _draw 发 1 个 draw_circle（%d/%d）" % [phase, circles, draws])
			ok(lines == 0 and polys == 0 and skips == 0, "相位 %d：不画线 / 多边形" % phase)
		elif kind == "line":
			n_line += 1
			ok(lines == draws, "相位 %d：每次 _draw 发 1 条 draw_line（%d/%d）" % [phase, lines, draws])
			ok(circles == 0 and polys == 0 and skips == 0, "相位 %d：不画圆 / 多边形" % phase)
		else:
			n_skip += 1
			ok(skips == draws, "相位 %d：顶点未复原 ⇒ 每次 _draw 记一次 skipped（%d/%d）"
				% [phase, skips, draws])
			ok(polys == 0, "相位 %d：没有顶点就**不调** draw_polygon（否则引擎会报错）" % phase)
		ok(int(node.call("process_call_count")) >= 1,
			"相位 %d：_process 被引擎调到（≥1）" % phase)
		node.queue_free()
	await process_frame
	ok(n_circle == 4 and n_line == 2 and n_skip == 1,
		"7 个相位分档：圆 4 个（0/2/3/6）、线 2 个（4/5）、多边形 1 个（1）—— 实际 %d/%d/%d"
			% [n_circle, n_line, n_skip])

	# ---- 多边形占位顶点：给了才画 ----
	var poly_node := await _fx_make(1)
	ok(int(poly_node.call("polygon_placeholder_count")) == 0, "默认没有占位顶点")
	poly_node.call("set_polygon_placeholder", PackedVector2Array([
		Vector2(-1.0, -1.0), Vector2(1.0, -1.0), Vector2(0.0, 1.0),
	]))
	ok(int(poly_node.call("polygon_placeholder_count")) == 3, "给了 3 个占位顶点")
	await process_frame
	await process_frame
	var pd: int = int(poly_node.call("draw_call_count"))
	ok(int(poly_node.call("drawn_polygon_total")) >= 1,
		"给了 3 个顶点之后真的调了 draw_polygon（%d 次，_draw 共 %d 次）"
			% [int(poly_node.call("drawn_polygon_total")), pd])
	ok(int(poly_node.call("drawn_polygon_total")) + int(poly_node.call("polygon_skipped_total")) == pd,
		"每次 _draw 要么画多边形、要么记 skipped，二者之和 == _draw 次数")
	poly_node.call("set_polygon_placeholder", PackedVector2Array([Vector2.ZERO]))
	ok(int(poly_node.call("polygon_placeholder_count")) == 0, "少于 3 个点等价于清空")
	poly_node.queue_free()
	await process_frame

	drawer.call("clear_effects")
	var dc: Control = drawer
	dc.queue_free()

# ---------------------------------------------------------------------------
# 2. GraphwarLongPressAnimation：两次 draw_arc + _process 推进
# ---------------------------------------------------------------------------

func _check_long_press() -> void:
	print("--- %s" % LPA)
	var anim: Object = _check_class(LPA, [
		"press", "release", "advance", "is_running", "elapsed", "angle",
		"spin_duration_secs", "draw_call_count", "process_call_count",
		"arc_total", "finished_count",
	])
	if anim == null:
		return

	ok(int(anim.get("long_press_ms")) == 500, "属性 long_press_ms 默认 500（+0x58 的长按阈值）")
	ok(int(anim.get("geometry_n")) == 0 and is_equal_approx(float(anim.get("geometry_c")), 0.0),
		"属性 geometry_n / geometry_c 默认 0（第三次取值语义未知）")
	ok(is_equal_approx(float(anim.call("spin_duration_secs")), 0.5),
		"spin_duration_secs() == 0.5（2π / 4π，与 500 ms 数值相同）")

	var c: Control = anim
	c.set_size(Vector2(60.0, 90.0))
	c.set_position(Vector2(10.0, 20.0))

	# 未启动：一个图元都不发。
	ok(bool(anim.call("is_running")) == false, "刚建出来 is_running() == false（nanos 是哨兵 1e9）")
	root.add_child(c)
	await process_frame
	await process_frame
	ok(int(anim.call("arc_total")) == 0, "没 press 时一条 draw_arc 都不发（原版 0x4d8115）")
	ok(int(anim.call("draw_call_count")) >= 1, "_draw 仍然被调到（它自己判 running）")

	# 启动 + 手动推进：每次 _draw 恒发 2 条弧。
	var arcs_before: int = int(anim.call("arc_total"))
	var draws_before: int = int(anim.call("draw_call_count"))
	anim.call("press")
	ok(bool(anim.call("is_running")) == true, "press() 之后 is_running() == true")
	ok(is_equal_approx(float(anim.call("elapsed")), 0.0), "press() 把 elapsed 清零")
	anim.call("advance", 0.1)
	ok(is_equal_approx(float(anim.call("elapsed")), 0.1), "advance(0.1) ⇒ elapsed == 0.1")
	print("      angle = %.6f（期望 0.1·4π = %.6f）"
		% [float(anim.call("angle")), 0.1 * 4.0 * PI])
	ok(is_equal_approx(float(anim.call("angle")), 0.1 * 4.0 * PI),
		"角度 == elapsed × 4π（0x4d8407 的 ×2/×2/×π）")
	await process_frame
	await process_frame
	var draws: int = int(anim.call("draw_call_count"))
	var arcs: int = int(anim.call("arc_total"))
	print("      draw +%d, arc +%d（期望 2 倍）"
		% [draws - draws_before, arcs - arcs_before])
	ok(draws - draws_before >= 1, "press 之后 _draw 又被调到了（+%d）" % (draws - draws_before))
	ok(arcs - arcs_before == 2 * (draws - draws_before),
		"运行中每次 _draw 发 2 条 draw_arc（+%d = 2×%d）"
			% [arcs - arcs_before, draws - draws_before])
	ok(int(anim.call("finished_count")) == 0, "0.1 s 还没到终点（0.5 s）")

	# 越过 2π：`_draw` 里写哨兵（原版 0x4d8503 在两次绘制**之后**）。
	anim.call("advance", 0.5)
	print("      elapsed = %.6f s（0.1 + 0.5 + 若干帧的 delta）" % float(anim.call("elapsed")))
	ok(float(anim.call("elapsed")) >= 0.6,
		"advance 之后 elapsed ≥ 0.6 s（帧 delta 也会叠上去）")
	ok(float(anim.call("angle")) > TAU, "角度越过 2π（%.6f > %.6f）"
		% [float(anim.call("angle")), TAU])
	await process_frame
	await process_frame
	ok(int(anim.call("finished_count")) >= 1, "越过 2π 之后 finished_count ≥ 1（哨兵已写）")
	ok(bool(anim.call("is_running")) == false, "结束之后 is_running() == false")
	var after_finish: int = int(anim.call("arc_total"))
	await process_frame
	await process_frame
	ok(int(anim.call("arc_total")) == after_finish, "结束之后再 _draw 也不发弧了（nanos == 1e9 直接返回）")

	# release() 等价 button_up。
	anim.call("press")
	ok(bool(anim.call("is_running")) == true, "再 press() 又能启动")
	anim.call("release")
	ok(bool(anim.call("is_running")) == false, "release() 写哨兵 ⇒ is_running() == false")

	c.queue_free()
	await process_frame


# ---------------------------------------------------------------------------
# 3. GraphwarPlane：3 个子节点 + 棋盘框 + 脏矩形
# ---------------------------------------------------------------------------

func _check_plane() -> void:
	print("--- %s" % PLANE)
	var plane: Object = _check_class(PLANE, [
		"build_children", "function_drawer_count", "to_pixel",
		"on_obstacle_partial_update", "partial_update_count", "drain_partial_updates",
		"ready_event_names", "on_resized",
		"draw_call_count", "drawn_rect_total", "process_call_count",
		"obstacle_update_total", "ready_call_count", "resized_call_count",
		"flushed_total", "dirty_redraw_count",
	])
	if plane == null:
		return

	var c: Control = plane
	c.set_size(Vector2(1000.0, 600.0))

	ok(int(plane.call("function_drawer_count")) == 0, "还没建子节点时 function_drawer_count() == 0")
	ok(int(plane.call("build_children")) == 3, "build_children() 返回 3（FUNCTION_DRAWER_CHILDREN）")
	ok(int(plane.call("function_drawer_count")) == 3, "function_drawer_count() == 3")
	ok(c.get_child_count() == 3, "子节点个数 == 3（实际 %d）" % c.get_child_count())
	ok(int(plane.call("build_children")) == 3, "build_children() 幂等（再调一次还是 3）")
	ok(c.get_child_count() == 3, "幂等：子节点还是 3 个（实际 %d）" % c.get_child_count())
	var kinds := PackedStringArray()
	for i in range(c.get_child_count()):
		kinds.append("%s(%s)" % [c.get_child(i).get_class(), c.get_child(i).name])
	print("      子节点: %s" % ", ".join(kinds))
	ok(c.get_child(0) is Control, "子节点是 Control（**占位**：FunctionDrawer 类还不存在）")

	# 像素换算（`to_pixel`：x*1000/50 + 500、y*1000/50 + 300）。
	ok(Vector2i(plane.call("to_pixel", 5.0, 5.0)) == Vector2i(600, 400),
		"to_pixel(5, 5) == (600, 400)")
	ok(Vector2i(plane.call("to_pixel", -25.0, -15.0)) == Vector2i(0, 0),
		"to_pixel(−25, −15) == (0, 0)（棋盘左下角 = 画布原点）")
	ok(Vector2i(plane.call("to_pixel", 25.0, 15.0)) == Vector2i(1000, 600),
		"to_pixel(25, 15) == (1000, 600)（右上角 = 画布右下角）")

	# 脏矩形：角点，不是 Rect2i。
	var rect: PackedInt32Array = plane.call("on_obstacle_partial_update", 0x28, 5.0, 5.0, 1.5)
	print("      脏矩形 = %s（期望 [570, 370, 630, 430]）" % str(rect))
	ok(Array(rect) == [570, 370, 630, 430],
		"on_obstacle_partial_update(0x28, 5, 5, 1.5) == [570, 370, 630, 430]")
	ok(int(plane.call("partial_update_count")) == 1, "partial_update_count() == 1")
	var bad: PackedInt32Array = plane.call("on_obstacle_partial_update", 0x99, 0.0, 0.0, 1.0)
	ok(bad.is_empty(), "坏 tag 0x99 返回空数组（原版这里 panic!，本类先拦）")
	ok(int(plane.call("partial_update_count")) == 1, "坏 tag 没有进队列")
	var events: PackedStringArray = plane.call("ready_event_names")
	ok(events.size() == 3 and events[0] == "add_obstacle_event"
			and events[1] == "destroy_obstacle_event"
			and events[2] == "clear_obstacles_event",
		"ready 会订的 3 个事件名（本类没订，只读名字）")

	# 进树：`_draw` 画 1 个框、`_process` 每帧刷 2 次。
	root.add_child(c)
	await process_frame
	await process_frame
	var draws: int = int(plane.call("draw_call_count"))
	print("      plane draw=%d rect=%d process=%d obstacle_updates=%d ready=%d"
		% [draws, int(plane.call("drawn_rect_total")), int(plane.call("process_call_count")),
			int(plane.call("obstacle_update_total")), int(plane.call("ready_call_count"))])
	ok(int(plane.call("ready_call_count")) >= 1, "_ready 被引擎调到")
	ok(draws >= 1, "_draw 被引擎调到（%d 次）" % draws)
	ok(int(plane.call("drawn_rect_total")) == draws,
		"每次 _draw 发 1 条 draw_rect（%d/%d）" % [int(plane.call("drawn_rect_total")), draws])
	var procs: int = int(plane.call("process_call_count"))
	ok(procs >= 1, "_process 被引擎调到（%d 次）" % procs)
	ok(int(plane.call("obstacle_update_total")) == 2 * procs,
		"每帧刷 2 次障碍绘制器（%d 次 / %d 帧）"
			% [int(plane.call("obstacle_update_total")), procs])
	ok(c.get_child_count() == 3, "进树之后 `ready` 没有重复建子节点（还是 3 个）")

	# 脏矩形 → 下一帧被 `_process` 冲掉 + 请求重画。
	plane.call("on_obstacle_partial_update", 0x29, -5.0, -5.0, 0.5)
	ok(int(plane.call("partial_update_count")) == 1, "队列里进了 1 个")
	await process_frame
	await process_frame
	ok(int(plane.call("partial_update_count")) == 0, "`_process` 把它冲掉了")
	ok(int(plane.call("flushed_total")) >= 1, "flushed_total ≥ 1（%d）" % int(plane.call("flushed_total")))
	ok(int(plane.call("dirty_redraw_count")) >= 1,
		"脏矩形触发过 queue_redraw（%d 次）" % int(plane.call("dirty_redraw_count")))

	# `resized` 真的连上了（信号名 / 处理者名都取自纯逻辑）。
	c.set_size(Vector2(500.0, 300.0))
	await process_frame
	await process_frame
	ok(int(plane.call("resized_call_count")) >= 1,
		"`resized` 连上了（on_resized 被调到 %d 次）" % int(plane.call("resized_call_count")))
	plane.call("on_resized")
	ok(int(plane.call("resized_call_count")) >= 2, "GDScript 也能直接调 on_resized")

	c.queue_free()
	await process_frame

# ---------------------------------------------------------------------------
# 4. PlayerShowcase：读档案 + 三层贴图 + 计数器闸门
# ---------------------------------------------------------------------------

func _check_showcase() -> void:
	print("--- %s" % SHOWCASE)
	var show: Object = _check_class(SHOWCASE, [
		"player_name", "skin", "face", "hat", "change_counter",
		"ready_steps", "name_edit_path", "on_text_change", "set_player_name",
		"layer_count", "available_layers", "build_demo_children", "demo_child_count",
		"touches_rank", "draw_call_count", "drawn_texture_rect_total",
		"redraw_request_count", "process_call_count", "ready_call_count",
		"root_lookup_failures",
	])
	if show == null:
		return

	ok(int(show.get("texture_size")) == 8, "属性 texture_size 默认 8")

	# 档案的三个默认外观名（`GraphwarPlayerConfig::new` 写进去的字面量）。
	ok(String(show.call("player_name")) == "", "档案里的名字默认空")
	ok(String(show.call("skin")) == "yellow", "皮肤默认 \"yellow\"（+0x60）")
	ok(String(show.call("face")) == "regular_eyes", "脸默认 \"regular_eyes\"（+0x78）")
	ok(String(show.call("hat")) == "soldier_helmet", "帽子默认 \"soldier_helmet\"（+0x90）")
	ok(int(show.call("change_counter")) == 0, "变更计数器默认 0（+0xa8）")
	ok(String(show.call("name_edit_path")) == "PlayerNameLineEdit",
		"名字控件名就是原版字面量 PlayerNameLineEdit")

	# 有界否定：不读段位。
	ok(bool(show.call("touches_rank")) == false, "touches_rank() == false（不读段位 / 队色，有界否定）")

	# 6 步 ready。
	var steps: PackedStringArray = show.call("ready_steps")
	print("      ready 的 6 步: %s" % ", ".join(steps))
	ok(steps.size() == 6, "ready_steps() 有 6 步（READY_PLAN）")
	ok(steps.size() > 0 and steps[0].contains("GraphwarRootNode"), "第 1 步是取 GraphwarRootNode")

	# 三层贴图：三个名字都非空 ⇒ 三层都画。
	ok(int(show.call("layer_count")) == 3, "layer_count() == 3（皮肤 → 脸 → 帽子）")
	ok(Array(show.call("available_layers")) == [1, 1, 1],
		"available_layers() == [1, 1, 1]")

	var c: Control = show
	c.set_size(Vector2(120.0, 160.0))
	show.call("set_player_name", "Bob")
	ok(String(show.call("player_name")) == "Bob", "set_player_name(\"Bob\") 生效")
	ok(int(show.call("change_counter")) == 0, "set_player_name 不经过 on_text_change ⇒ 计数器不动")

	# 进树：`_ready` 建控件、灌名字、连信号。
	root.add_child(c)
	await process_frame
	await process_frame
	ok(int(show.call("ready_call_count")) >= 1, "_ready 被引擎调到")
	ok(int(show.call("root_lookup_failures")) >= 1,
		"第 1 步找不到 GraphwarRootNode（本类没有那个节点；原版这里会 .expect() 失败）")
	var edit: LineEdit = c.get_node_or_null("PlayerNameLineEdit")
	ok(edit != null, "ready 建/找到了 PlayerNameLineEdit 子节点")
	if edit != null:
		ok(String(edit.text) == "Bob", "ready 第 4 步把档案里的名字灌进了控件（\"%s\"）" % edit.text)
		ok(edit.get_signal_connection_list("text_changed").size() >= 1,
			"ready 第 6 步把 text_changed 连到了 on_text_change")

	# `_draw`：三层贴图（每次 _draw 恒 3 条 draw_texture_rect）。
	var draws: int = int(show.call("draw_call_count"))
	var rects: int = int(show.call("drawn_texture_rect_total"))
	print("      showcase draw=%d texture_rects=%d process=%d"
		% [draws, rects, int(show.call("process_call_count"))])
	ok(draws >= 1, "_draw 被引擎调到（%d 次）" % draws)
	ok(rects == 3 * draws, "每次 _draw 发 3 条 draw_texture_rect（%d = 3×%d）" % [rects, draws])

	# 计数器闸门：闸门状态（last_counter）初始 0 且档案也是 0 ⇒ 不重画。
	var redraws_before: int = int(show.call("redraw_request_count"))
	await process_frame
	await process_frame
	ok(int(show.call("redraw_request_count")) == redraws_before,
		"计数器没变 ⇒ _process 不请求重画（%d）" % redraws_before)

	# `on_text_change`：名字改回档案 + 计数器 +1（原版 `0x448e40`）。
	ok(int(show.call("on_text_change", "Alice")) == 1, "on_text_change(\"Alice\") 返回新的计数器 1")
	ok(String(show.call("player_name")) == "Alice", "名字被改成 Alice")
	ok(int(show.call("change_counter")) == 1, "变更计数器 +1 == 1")
	await process_frame
	await process_frame
	ok(int(show.call("redraw_request_count")) >= 1,
		"计数器变了 ⇒ _process 请求重画（%d 次）" % int(show.call("redraw_request_count")))

	# LineEdit 的信号链：改文本 ⇒ `on_text_change` ⇒ 计数器再 +1。
	if edit != null:
		# ⚠️ 本轮实测：Godot 4.7 的 `LineEdit.set_text()`（属性 `text = ...` **不**发
		# `text_changed`（只有用户编辑才发）。headless 里没有键盘 ⇒ 手动 `emit_signal`
		# 打一发，验的是**连线**对不对，而不是"按键事件能不能走通"。
		edit.text = "Carol"
		await process_frame
		ok(String(show.call("player_name")) == "Alice",
			"只改控件文字**不**触发（set_text 不发 text_changed，实测）")
		edit.emit_signal("text_changed", "Carol")
		await process_frame
		ok(String(show.call("player_name")) == "Carol",
			"控件 text_changed → on_text_change ⇒ 档案名字变成 Carol（实际 \"%s\"）"
				% String(show.call("player_name")))
		ok(int(show.call("change_counter")) == 2, "计数器再 +1 == 2")

	# 空外观名 ⇒ 那一层没贴图 ⇒ 不画（与 `draw_plan` 的 `if let Some(t)` 同构）。
	ok(int(show.call("layer_count")) == 3, "清空之前还是 3 层")

	# 任务书要的 3 Sprite2D + 1 标签（**非原版**，opt-in 的演示口）。
	var before: int = c.get_child_count()
	ok(int(show.call("build_demo_children")) == 4, "build_demo_children() 返回 4（3 Sprite2D + 1 Label）")
	ok(int(show.call("demo_child_count")) == 4, "demo_child_count() == 4")
	ok(c.get_child_count() == before + 4,
		"子节点多了 4 个（%d → %d）" % [before, c.get_child_count()])
	var sprites := 0
	var labels := 0
	var sprite_names := PackedStringArray()
	for i in range(c.get_child_count()):
		var kid: Node = c.get_child(i)
		if kid is Sprite2D:
			sprites += 1
			sprite_names.append(String(kid.name))
		elif kid is Label:
			labels += 1
	print("      demo 子节点: sprites=%s label=%d" % [", ".join(sprite_names), labels])
	ok(sprites == 3, "演示子节点里有 3 个 Sprite2D（实际 %d）" % sprites)
	ok(labels == 1, "演示子节点里有 1 个 Label（实际 %d）" % labels)
	ok(int(show.call("build_demo_children")) == 4, "build_demo_children() 幂等")
	ok(c.get_child_count() == before + 4, "幂等：没有重复建（还是 %d 个）" % c.get_child_count())

	# GDScript 的 `.new()`：第三条实例化路径。
	var fresh: Object = PlayerShowcase.new()
	ok(fresh != null and fresh is Control, "PlayerShowcase.new() 出来的是 Control")
	if fresh != null:
		var fw: Control = fresh
		ok(String(fw.call("skin")) == "yellow", "新实例的档案默认值同样是 \"yellow\"")
		fw.free()

	c.queue_free()
	await process_frame

# ---------------------------------------------------------------------------
# 入口
# ---------------------------------------------------------------------------

func _initialize() -> void:
	# 0) 扩展加载了吗？第一次跑到（没有 `.godot/extension_list.cfg`）时不会，显式补一次。
	if not ClassDB.class_exists(FX):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	ok(ClassDB.class_exists("GW2Rebuild"), "对照组：单例 GW2Rebuild 仍在（没有被这次改动弄坏）")
	ok(ClassDB.class_exists("AxisDrawer"), "对照组：AxisDrawer 仍在（§35 的类）")
	ok(ClassDB.class_exists("EntityDrawer"), "对照组：EntityDrawer 仍在（§38 的类）")

	await _check_effect_drawer()
	await _check_long_press()
	await _check_plane()
	await _check_showcase()

	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("DRAW_CLASSES %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
