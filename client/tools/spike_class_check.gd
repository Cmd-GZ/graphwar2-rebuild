extends SceneTree
## `AxisDrawer` 类注册 spike 的验证脚本（`docs/16` §35）。
##
## ```text
## bash client/run.sh spike-class
## ```
##
## 等价于直接跑：
##
## ```text
## godot --headless --path client --script res://tools/spike_class_check.gd
## ```
##
## 只验证四件事，**不验证绘制**（虚方法表还没接）：
##
## 1. 类真的在 `ClassDB` 里（`class_exists` 为真）；
## 2. 三条实例化路径都通：`ClassDB.instantiate` / `.tscn` 场景 / GDScript 的 `AxisDrawer.new()`；
## 3. 属性 `axis_mode` 能读能写，`get_property_list` 里看得见；
## 4. 两个"数线"方法（图元枚举 vs 闭式公式）在 0/1/2/3 与越界值上 **对拍一致** ——
##    这一步证明 Rust 侧 `src/axis_drawer.rs` 的复原逻辑**真的被引擎调到了**。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const CLASS_NAME := "AxisDrawer"
const SCENE_PATH := "res://tools/spike_axis_drawer.tscn"
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
	# 0) 扩展有没有被加载过？`--script` 模式下 Godot 一般已经把项目登记的
	#    `.gdextension` 加载了；万一没有（或本脚本被单独跑），显式加载一次。
	var gw2_before := ClassDB.class_exists("GW2Rebuild")
	print("      before load: GW2Rebuild=%s AxisDrawer=%s" % [gw2_before, ClassDB.class_exists(CLASS_NAME)])
	if not ClassDB.class_exists(CLASS_NAME):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	ok(ClassDB.class_exists(CLASS_NAME), "ClassDB.class_exists(\"%s\")" % CLASS_NAME)
	ok(ClassDB.class_exists("GW2Rebuild"), "对照组：单例 GW2Rebuild 仍在（没有被这次改动弄坏）")

	# 1) ClassDB.instantiate：`.tscn` 的 `type="AxisDrawer"` 与 `new()` 走的都是这条路径。
	var inst: Object = ClassDB.instantiate(CLASS_NAME)
	ok(inst != null, "ClassDB.instantiate(\"%s\") 非空" % CLASS_NAME)
	if inst == null:
		_finish()
		return
	ok(inst is Control, "实例能当 Control 用（父类正确）")
	ok(inst.is_class(CLASS_NAME), "inst.is_class(\"%s\") 为真" % CLASS_NAME)
	print("      engine facts: get_class()=%s is_class(Control)=%s" % [inst.get_class(), inst.is_class("Control")])

	# 2) 属性：在 get_property_list 里、能读写、setter/getter 方法也一致。
	ok(_has_property(inst, "axis_mode"), "get_property_list() 里有 axis_mode")
	var default_mode: int = int(inst.get("axis_mode"))
	ok(default_mode == 0, "axis_mode 默认值 = 0（NoAxis；我方选择，非二进制证据）")
	inst.set("axis_mode", 3)
	ok(int(inst.get("axis_mode")) == 3, "set/get axis_mode=3 往返一致")
	ok(int(inst.call("get_axis_mode")) == 3, "get_axis_mode() 与属性读一致")

	# 3) 两条"数线"路径全对拍：图元枚举（axis_primitives）vs 闭式公式（line_count）。
	#    期望值 0 / 2 / 20 / 102 见 docs/16 §21。
	var expected := {0: 0, 1: 2, 2: 20, 3: 102}
	for mode in [0, 1, 2, 3]:
		inst.call("set_axis_mode", mode)
		var by_prims: int = int(inst.call("axis_line_count"))
		var by_formula: int = int(inst.call("axis_line_count_formula"))
		ok(
			by_prims == expected[mode] and by_formula == expected[mode],
			"axis_mode=%d -> 枚举 %d 条 / 公式 %d 条（期望 %d）"
				% [mode, by_prims, by_formula, expected[mode]]
		)
	# 越界判别值：原版只比较 ==3 / >=2 / !=0，所以 200 应当画"粗网格 + 主轴" = 20 条。
	inst.call("set_axis_mode", 200)
	ok(int(inst.call("axis_line_count")) == 20, "axis_mode=200（越界）→ 20 条（原版分支）")
	inst.call("set_axis_mode", 3)

	# 4) 场景实例化：`.tscn` 里 `type="AxisDrawer"` 由**场景加载器**按名解析。
	var packed: PackedScene = load(SCENE_PATH)
	ok(packed != null, "载入 %s" % SCENE_PATH)
	if packed != null:
		var node: Node = packed.instantiate()
		ok(node != null, "场景能实例化出节点")
		if node != null:
			ok(node is Control, "场景节点是 Control")
			ok(node.is_class(CLASS_NAME), "场景节点 is_class(AxisDrawer)")
			# `.tscn` 里写了 `axis_mode = 3` ⇒ 场景加载器把属性写进去了。
			ok(
				int(node.call("axis_line_count_formula")) == 102,
				".tscn 里的 axis_mode=3 生效（102 条线）"
			)
			# 进树这一步要等到**一帧之后**再判：`--script` 模式下 `_initialize()` 跑在
			# `SceneTree` 自己的 root 进树之前，那一刻 `add_child` 只是记下父子关系，
			# `is_inside_tree()` 还不会为真（这是引擎时序，不是我们的 bug）。
			root.add_child(node)
			print("      add_child 后立刻查: root.is_inside_tree=%s node.is_inside_tree=%s"
				% [root.is_inside_tree(), node.is_inside_tree()])
			await process_frame
			ok(node.is_inside_tree(), "节点能挂进场景树（Node 生命周期可用）")
			node.queue_free()

	# 5) GDScript 里直接写 `AxisDrawer.new()`：解析期就认得这个类名（运行时注册的类
	#    也会进 GDScript 的原生类表）。
	var fresh = AxisDrawer.new()
	ok(fresh != null, "AxisDrawer.new() 非空")
	if fresh != null:
		ok(fresh is Control, "AxisDrawer.new() 出来的是 Control")
		fresh.call("set_axis_mode", 2)
		ok(int(fresh.call("axis_line_count")) == 20, "新实例的 axis_mode=2 → 20 条")
		fresh.free()

	await process_frame
	inst.free()
	_finish()

## `get_property_list()` 里有没有这个属性（顺带核对 `usage` 带 STORAGE 位，
## 否则 `.tscn` 里的值不会被保存/加载）。
func _has_property(obj: Object, name: String) -> bool:
	for prop in obj.get_property_list():
		if String(prop.get("name", "")) == name:
			var usage: int = int(prop.get("usage", 0))
			print("      property %s: type=%s usage=%d" % [name, prop.get("type"), usage])
			return (usage & PROPERTY_USAGE_STORAGE) != 0
	return false

func _finish() -> void:
	var passed := _checks - _failures.size()
	print("SPIKE %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
