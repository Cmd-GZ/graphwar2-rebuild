# fixtures/t88_active_window_probe.gd —— T88 的**机制判据**（2026-11-27，任务 task-84）
#
# 这不是客户端的一部分：它是"活跃窗口语义"的确定性探针，**不入 headless 14 脚本门槛**
# （文件名不是 `*_check.gd`）。跑法（需要先 `cargo build --features gdext-bindings` +
# `cp libgw2_rebuild.so client/` + `godot --headless --path client --import`）：
#
#   cp fixtures/t88_active_window_probe.gd client/tools/t88_probe.gd
#   $GODOT --headless --path client --script res://tools/t88_probe.gd    # 期望 fails=0
#   rm client/tools/t88_probe.gd client/tools/t88_probe.gd.uid
#
# 判据（全部来自反汇编，不来自实现自证；`docs/06` §137）：
#   * `drive_frame(0.5)` ⇒ tick 30（定步 `16_666_666 ns`，`0x519840`）；
#   * `set_fractional_tick(550)` = 把轴对齐到服务端 tick（`TickReply` 那条路）；
#   * `FunctionPoints` 先到、`FunctionActive{active_tick=615}` 后到 ⇒ entry 仍在、
#     起飞 tick 变成 615（`0x456b80`；`0x4b6418` 只删 `Line2D` 不删管理器的 entry）；
#   * 在场谓词 `615 < tick`（严格）且 `30·d < len+1800`（`0x3be5f6`–`0x3be613`）；
#   * 揭示 `30 点/tick`（`0x4b67a0`）、淡出 `max(0, 1-(d-len/30)/60)`（`0x455a40`）。
# 口径照着**原版脚本化的对局剧本**来：本地 tick 已到 550 时收到 FunctionPoints（active_tick
# 还没到 ⇒ 0），随后 FunctionActive{active_tick=615} 迟到 —— 这是 `fixtures/frame_match_events.txt`
# [phase2] 的真实顺序。
extends SceneTree
var _fails := 0
## 某条曲线在 `Line2D` 上已经有几个点（读 `FunctionDrawer::point_count`，与 `_curve_reveal()` 同源）。
func _line_points(plane: Node, pid: int) -> int:
	for fd in plane.get_children():
		if String(fd.get_class()) == "FunctionDrawer":
			return maxi(0, int(fd.call("point_count", pid)))
	return -1

func ok(cond: bool, what: String) -> void:
	if cond:
		print("  ok   ", what)
	else:
		_fails += 1
		print("  FAIL ", what)

func _initialize() -> void:
	print("== T88 探针 ==")
	var plane: Node = ClassDB.instantiate("GraphwarPlane")
	root.add_child(plane)
	plane.call("build_children")

	# ① drive_frame 定步：0.5 s ⇒ 30 tick；再 1.0 s ⇒ 90
	var t1 := float(plane.call("drive_frame", 0.5))
	ok(abs(t1 - 30.0) < 0.01, "drive_frame(0.5) ⇒ tick 30（定步 16_666_666 ns）")
	var t2 := float(plane.call("drive_frame", 1.0))
	ok(abs(t2 - 90.0) < 0.01, "drive_frame(1.0) ⇒ tick 90")

	# ② 时钟对齐到服务端 tick 轴（TickReply 那条路）
	plane.call("set_fractional_tick", 550.0)
	ok(abs(float(plane.call("fractional_tick")) - 550.0) < 1e-6, "set_fractional_tick(550) ⇒ 读数 550")

	# ③ FunctionPoints 先到：注入 780 点，active_tick 还是默认 0
	var pts := PackedFloat64Array()
	for i in 780:
		pts.push_back(float(i) * 0.01)
		pts.push_back(0.0)
	ok(int(plane.call("set_function_points", 230, pts, 0, 0x00ff0000)) == 780, "注入 780 点")
	plane.call("feed_functions")
	# 860 点 ⇒ 窗口 = len/30 + 60 = 86；d = 550（active=0）⇒ 30·550 = 16500 ≫ 780+1800 ⇒ 不在场
	ok(int(plane.call("function_count")) == 1, "entry 还在管理器里（不在场只删线，不删 entry）")
	print("    debug: line_points@550=", _line_points(plane, 230), " drawers=", plane.call("function_drawer_count"))
	ok(_line_points(plane, 230) == 0, "tick=550/active=0 ⇒ 超窗口，没画线")

	# ④ FunctionActive 迟到：起飞 tick 615 写进去（entry 还在 ⇒ 生效）
	var w0 := int(plane.call("function_active_tick_writes", 230))
	ok(int(plane.call("set_function_active_tick", 230, 615)) == 1, "set_function_active_tick 命中 entry")
	ok(int(plane.call("function_active_tick_of", 230)) == 615, "active_tick ⇒ 615（FunctionActive 真值）")
	ok(int(plane.call("function_active_tick_writes", 230)) == w0 + 1, "写入计数 +1（来路可证，不是恒 0）")

	# ⑤ 窗口：615 不揭示；616 揭 30 点；641 全揭 780 点
	var rev_at := func(tick: float) -> int:
		plane.call("set_fractional_tick", tick)
		plane.call("feed_functions")
		return _line_points(plane, 230)
	ok(rev_at.call(615.0) == 0, "tick==active_tick ⇒ 0 点（严格大于）")
	ok(rev_at.call(616.0) == 30, "tick=616 ⇒ 30 点（每 tick 30 点）")
	ok(rev_at.call(641.0) == 780, "tick=641（26 tick）⇒ 780 点全揭")

	# ⑥ 淡出：d=26 ⇒ 1.0；d=27 ⇒ 1-1/60；d=86 ⇒ 0.0
	var fade_at := func(tick: float) -> float:
		plane.call("set_fractional_tick", tick)
		plane.call("feed_functions")
		return float(plane.call("last_fade_alpha"))
	ok(abs(fade_at.call(641.0) - 1.0) < 1e-7, "alpha@d=26 ⇒ 1.0（还在揭示）")
	print("    debug: fade@642=", fade_at.call(642.0))
	ok(abs(fade_at.call(642.0) - (1.0 - 1.0/60.0)) < 1e-7, "alpha@d=27 ⇒ 1-1/60（线性淡出 60 tick；返回值过 f32）")
	# d=86 时 30·86 = 2580 = 780+1800 ⇒ **已经出窗口**，那一帧不再调 update_function_fade
	# （`last_fade_alpha` 保留 d=85 的值）。窗口边界与淡出终点恰好重合：d=85 ⇒ 1-59/60。
	ok(abs(fade_at.call(700.0) - (1.0 - 59.0/60.0)) < 1e-7, "alpha@d=85 ⇒ 1-59/60")
	ok(_line_points(plane, 230) > 0, "tick=700（d=85）⇒ 线还在场（只是 alpha 很低）")
	plane.call("set_fractional_tick", 701.0)
	plane.call("feed_functions")
	ok(_line_points(plane, 230) == 0, "tick=701（d=86）⇒ 出窗口，线已删")

	# ⑦ 出窗口（d=87）⇒ 线被删；entry 仍留着
	plane.call("set_fractional_tick", 702.0)
	plane.call("feed_functions")
	ok(_line_points(plane, 230) == 0, "d=87 ⇒ 线仍不在（0x4b6418 只删一次）")
	ok(int(plane.call("function_count")) == 1, "entry 仍在（原版管理器不删）")

	print("== fails=%d ==" % _fails)
	quit(0 if _fails == 0 else 1)
