extends SceneTree
## 官方 gdext 绑定层的验收脚本（`docs/18-build.md` §10）。
##
## ```text
## cargo build --release --features gdext-bindings
## cp target/release/libgw2_rebuild.so client/            # 覆盖手写版（先备份！）
## godot --headless --path client --script res://tools/gdext_check.gd
## ```
##
## 它只验证"**换成 gdext 之后客户端还能用**"这一句，不验证复原逻辑：
##
## 1. 类 `GW2Rebuild` 在 `ClassDB` 里，`ping()` 有返回值；
## 2. 五个方法（`ping` / `path_calculate` / `path_calculate_mode` / `hit_test` /
##    `level_obstacles`）都还在，`path_calculate` 真出点、`hit_test` 真判、`level_obstacles` 真生成；
## 3. 真节点类 `AxisDrawer` 能实例化、属性可读写、枚举法与公式法数线一致；
## 4. `_draw` 有没有被引擎调到（只有 gdext 版有 `draw_call_count()` 这个观测点；
##    手写版没有虚方法表，也就没有这件事）。
##
## 与 `client/tools/spike_class_check.gd` 的区别：那个测的是**手写注册**能不能注册成功，
## 这个测的是**换成 gdext 之后契约有没有变**。两者都跑，就能证明"换绑定层对客户端透明"。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const KERNEL_CLASS := "GW2Rebuild"
const NODE_CLASS := "AxisDrawer"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

func _initialize() -> void:
	# 0) 扩展加载了吗？`--script` 模式下 Godot 一般已经加载了项目登记的 `.gdextension`，
	#    第一次跑（没有 `.godot/extension_list.cfg`）时不会，显式补一次。
	print("      before load: GW2Rebuild=%s AxisDrawer=%s"
		% [ClassDB.class_exists(KERNEL_CLASS), ClassDB.class_exists(NODE_CLASS)])
	if not ClassDB.class_exists(KERNEL_CLASS):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	ok(ClassDB.class_exists(KERNEL_CLASS), "ClassDB.class_exists(\"%s\")" % KERNEL_CLASS)
	if not ClassDB.class_exists(KERNEL_CLASS):
		_finish()
		return

	var kernel: Object = ClassDB.instantiate(KERNEL_CLASS)
	ok(kernel != null, "ClassDB.instantiate(\"%s\") 非空" % KERNEL_CLASS)
	if kernel == null:
		_finish()
		return
	print("      kernel.get_class() = %s" % kernel.get_class())

	# 1) 五个方法都在（客户端 `core_bridge.gd` 就是按名字探测的）。
	var wanted := [
		"ping", "path_calculate", "path_calculate_mode", "hit_test", "level_obstacles",
	]
	var names := PackedStringArray()
	for m in kernel.get_method_list():
		names.append(String(m.name))
	for w in wanted:
		ok(names.has(w), "内核方法 %s() 存在" % w)

	# 2) ping：手写版是空方法（返回 null），gdext 版返回 "pong:gdext"。
	#    这里只要求"能调通"，返回值原样打印出来当作绑定层指纹。
	var pong: Variant = kernel.call("ping")
	print("      ping() -> %s（%s）" % [pong, type_string(typeof(pong))])
	# 手写版的 `ping` 是空方法（`has_return_value = 0` ⇒ 返回 null），gdext 版返回字符串。
	# 这个脚本要能对**两份绑定**都跑通，所以判据是"调得通"，返回值只当指纹打印。
	ok(pong == null or (pong is String and not String(pong).is_empty()),
		"ping() 调得通（手写版 null / gdext 版 \"pong:gdext\"）")
	if kernel.has_method("binding_version"):
		print("      binding_version() -> %s" % kernel.call("binding_version"))

	# 3) path_calculate：y = x 从原点向右飞，点数应远大于 2。
	var path: Dictionary = kernel.call("path_calculate", "x", 0.0, 0.0, 1.0)
	ok(bool(path.get("ok", false)), "path_calculate(\"x\", 0, 0, +1).ok")
	var pts: PackedVector2Array = path.get("points", PackedVector2Array())
	ok(pts.size() > 2, "path_calculate 出点：%d 个" % pts.size())
	if pts.size() > 1:
		print("      first=%s last=%s offset=%s"
			% [pts[0], pts[pts.size() - 1], path.get("offset")])

	# 4) path_calculate_mode：normal 必须与 path_calculate 逐点一致，ode 要能单独跑通。
	var path_n: Dictionary = kernel.call("path_calculate_mode", "x", 0.0, 0.0, 1.0, "normal")
	var pts_n: PackedVector2Array = path_n.get("points", PackedVector2Array())
	ok(bool(path_n.get("ok", false)) and pts_n.size() == pts.size(),
		"path_calculate_mode(...,\"normal\") 与 path_calculate 点数一致（%d）" % pts_n.size())
	var path_ode: Dictionary = kernel.call("path_calculate_mode", "x + y", 0.0, 0.0, 1.0, "ode")
	ok(bool(path_ode.get("ok", false)), "path_calculate_mode(...,\"ode\") 能算（dy/dx = x + y）")
	var bad_mode: Dictionary = kernel.call("path_calculate_mode", "x", 0.0, 0.0, 1.0, "nope")
	ok(not bool(bad_mode.get("ok", true)) and String(bad_mode.get("error", "")) == "bad_arguments",
		"未知 mode 走 bad_arguments（不猜默认值）")

	# 5) hit_test：用上面那条弹道打一个站在 (5, 5)、半径 0.5 的士兵。
	var soldiers := PackedFloat64Array([5.0, 5.0, 0.5])
	var obstacles := PackedFloat64Array()
	var hit: Dictionary = kernel.call("hit_test", pts, obstacles, soldiers)
	ok(bool(hit.get("ok", false)), "hit_test(...).ok")
	print("      hit_test -> hits=%s first_hit=%s blocked=%s stop=%s"
		% [hit.get("hits"), hit.get("first_hit"), hit.get("blocked"), hit.get("stop")])
	ok(Array(hit.get("hits", [])).has(0), "弹道穿过 (5,5) 的士兵 ⇒ hits 里有下标 0")

	# 6) level_obstacles：21 关都调一遍，数组步长必须是 3（与 hit_test 的入参同一种数组）。
	#    第 0 关是教学关、本来就没有障碍，所以判据是"全部 ok 且至少一关非空"。
	var total := 0
	var nonempty: PackedInt32Array = PackedInt32Array()
	var all_ok := true
	for level in range(21):
		var one: Dictionary = kernel.call("level_obstacles", level, 12345)
		all_ok = all_ok and bool(one.get("ok", false))
		var flat_one: PackedFloat64Array = one.get("obstacles", PackedFloat64Array())
		if flat_one.size() % 3 != 0:
			all_ok = false
		if flat_one.size() > 0:
			nonempty.append(level)
			total += flat_one.size() / 3
	ok(all_ok, "level_obstacles(0..20, 12345) 全部 ok，且扁平数组都是 3 的倍数")
	ok(nonempty.size() > 0, "至少一关有障碍：%d 关 / 共 %d 个（关卡号 %s）"
		% [nonempty.size(), total, nonempty])
	var obs_bad: Dictionary = kernel.call("level_obstacles", 999, 0)
	ok(not bool(obs_bad.get("ok", true)), "越界关卡号 ⇒ ok=false（不崩）")

	# 7) AxisDrawer：真节点类 + 属性 + 数线（期望值 0 / 2 / 20 / 102，见 `docs/16` §21）。
	ok(ClassDB.class_exists(NODE_CLASS), "ClassDB.class_exists(\"%s\")" % NODE_CLASS)
	var node: Object = ClassDB.instantiate(NODE_CLASS)
	ok(node != null and node is Control, "AxisDrawer 能实例化且是 Control")
	if node != null:
		node.set("axis_mode", 3)
		ok(int(node.get("axis_mode")) == 3, "属性 axis_mode=3 往返一致")
		ok(int(node.call("axis_line_count")) == 102 and int(node.call("axis_line_count_formula")) == 102,
			"axis_mode=3 ⇒ 枚举 102 条 / 公式 102 条")
		node.set("axis_mode", 1)
		ok(int(node.call("axis_line_count")) == 2, "axis_mode=1 ⇒ 2 条（只画两条主轴）")
		node.set("axis_mode", 3)

		# 8) `_draw`：只有 gdext 版有计数（手写版没有虚方法表）。
		if node.has_method("draw_call_count"):
			root.add_child(node)
			node.queue_redraw()
			await process_frame
			await process_frame
			var calls: int = int(node.call("draw_call_count"))
			var lines: int = int(node.call("drawn_line_total"))
			print("      _draw 观测: draw_call_count=%d drawn_line_total=%d" % [calls, lines])
			if calls > 0:
				ok(lines >= 102, "_draw 真的发了线（%d 条 ≥ 102）" % lines)
			else:
				print("      注意：headless 下引擎没有回调 _draw（不是绑定层的问题）")
			root.remove_child(node)
		node.free()

	_finish()

func _finish() -> void:
	var passed := _checks - _failures.size()
	print("GDEXT %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
