extends SceneTree
## 三个"要信号接线"的类的验收脚本（`docs/16-client.md` §39）。
##
## ```text
## cargo build --release --features gdext-bindings
## cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
## godot --headless --path client --import
## godot --headless --path client --script res://tools/signal_classes_check.gd
## ```
##
## 它验的是**三条机制**，不是复原逻辑本身：
##
## 1. **自造 `#[signal]`**：`has_signal` / `get_signal_list` 认不认；GDScript
##    `node.connect("自造名", cb)` 收得到收不到；两条发信号的路径都试
##    （`GraphwarButton` 用类型安全生成器、`GraphwarFunctionInput` 用裸 `emit_signal`）；
## 2. **连引擎原生信号**：`is_connected(signal, Callable(自己, 处理函数))` 是不是真；
##    三种形态都试 —— 连**自己的**（`BaseButton` 的 3 条）、连**自己的**
##    （`LineEdit` 的 3 条）、连**子节点的**（`GraphwarSetupControl` 的 3 条）；
## 3. **外部注册闭包表**：`Vec<Box<dyn FnMut(&str)>>` 这个类型在 Rust 侧到底能不能用
##    （`subscribe_native_probe` 塞一个探针闭包，看它收不收到文本），以及 GDScript
##    能不能把闭包交给它（不能 —— 改用 `Callable`，这一条也是断言的，见第 4 段）。
##
## 与 `entity_drawer_check.gd` 的分工：那个验「纯逻辑 → 节点树」，这个验「信号/回调」。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const EXTENSION_PATH := "res://gw2rebuild.gdextension"

const BUTTON_CLASS := "GraphwarButton"
const INPUT_CLASS := "GraphwarFunctionInput"
const SETUP_CLASS := "GraphwarSetupControl"

## 观测到的信号/计数器（成员的写法是为了让 lambda 能写回来）。
var _lp_finished := 0
var _input_selected: Array = []
var _expression_changed: Array = []
var _function_submitted: Array = []
var _options_changed: Array = []
var _start_requested := 0
var _exit_requested := 0
var _chat_message: Array = []
## `subscribe(Callable)` 那个替代方案收到的文本（机制 3 的对照组）
var _callable_hits: Array = []
## `_probe_virtual_call` 造出来、然后被 SCRIPT ERROR 打断的那个对象（留着最后统一 free）
var _probe_leftover: Object = null

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

## 方法表里有没有这个名字。
func _has_method(obj: Object, method: String) -> bool:
	for m in obj.get_method_list():
		if String(m.name) == method:
			return true
	return false

## 某个方法的参数类型（`type_string` 的返回值），例如 `["Callable"]` / `[]`。
##
## 第 4 段用它证明「GDScript 只能给 `Callable`」—— 这是**引擎侧的证据**，不是猜测。
func _arg_types(obj: Object, method: String) -> Array:
	for m in obj.get_method_list():
		if String(m.name) == method:
			var out: Array = []
			for a in m.args:
				out.append(type_string(int(a.type)))
			return out
	return []

## 信号表的全部名字。
func _signal_names(obj: Object) -> PackedStringArray:
	var out := PackedStringArray()
	for s in obj.get_signal_list():
		out.append(String(s.name))
	return out

## 属性表里有没有这个名字（`#[export]` 才进得来，`#[var]` 也进；纯 Rust 字段进不来）。
func _has_property(obj: Object, prop: String) -> bool:
	for p in obj.get_property_list():
		if String(p.name) == prop:
			return true
	return false

func _initialize() -> void:
	# 0) 扩展加载（第一次跑到、没有 `.godot/extension_list.cfg` 时不会自动加载）
	if not ClassDB.class_exists(BUTTON_CLASS):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	for cls in [BUTTON_CLASS, INPUT_CLASS, SETUP_CLASS]:
		ok(ClassDB.class_exists(cls), "ClassDB.class_exists(\"%s\")" % cls)

	await _check_button()
	await _check_function_input()
	await _check_setup_control()
	_check_mechanism3_limits()
	_probe_virtual_call()

	_finish()

# ===========================================================================
# A. GraphwarButton：机制 1 + 2（自己的原生信号）+ 3（原版形态的闭包表）
# ===========================================================================
func _check_button() -> void:
	print("--- A. %s ---" % BUTTON_CLASS)
	if not ClassDB.class_exists(BUTTON_CLASS):
		return

	var btn: Object = ClassDB.instantiate(BUTTON_CLASS)
	ok(btn != null and btn is Button, "%s 能实例化且是 Button" % BUTTON_CLASS)
	if btn == null:
		return
	print("      btn.get_class() = %s" % btn.get_class())
	ok(btn.is_class("BaseButton"), "is_class(\"BaseButton\") 为真（父类正确）")

	# --- 方法表：原版 6 个方法里能被 #[func] 暴露的 3 个 + 观测点 ---
	for method in ["pressed", "button_down", "button_up", "subscribe", "subscribe_native_probe"]:
		ok(_has_method(btn, method), "方法 %s() 存在" % method)

	# --- 导出属性（`#[export] GString`：§38.6 标"未验"的那一项）---
	btn.set("input_text", "X")
	ok(String(btn.get("input_text")) == "X", "属性 input_text 往返一致（\"X\"）")
	btn.set("long_press_input_text", "S")
	ok(String(btn.get("long_press_input_text")) == "S", "属性 long_press_input_text 往返一致（\"S\"）")

	# --- 机制 1：自造信号注册了吗 ---
	var sigs: PackedStringArray = _signal_names(btn)
	ok(sigs.has("long_press_finished"), "自造信号 long_press_finished 在 get_signal_list 里")
	ok(sigs.has("input_selected"), "自造信号 input_selected 在 get_signal_list 里")
	# 基类信号仍在（自造信号没有顶掉别人的）
	ok(sigs.has("pressed") and sigs.has("button_down") and sigs.has("button_up"),
		"基类 BaseButton 的 pressed/button_up/button_down 三条信号还在")
	# `input_selected` 的 1 个参数类型是 String
	for s in btn.get_signal_list():
		if String(s.name) == "input_selected":
			ok(s.args.size() == 1 and type_string(int(s.args[0].type)) == "String",
				"自造信号 input_selected 带 1 个 String 参数（实际 %s）" % str(s.args.size()))

	# --- 机制 1：GDScript 侧 connect 自造信号 ---
	var e1: int = btn.connect("long_press_finished", func() -> void: _lp_finished += 1)
	ok(e1 == 0, "GDScript connect(\"long_press_finished\", cb) 返回 OK（%d）" % e1)
	var e2: int = btn.connect("input_selected", func(text: String) -> void: _input_selected.append(text))
	ok(e2 == 0, "GDScript connect(\"input_selected\", cb) 返回 OK（%d）" % e2)

	# --- 机制 3：两张表各登记一个 ---
	ok(int(btn.call("subscribe_native_probe")) == 1, "subscribe_native_probe() 让 subscriber_count 变 1")
	ok(int(btn.call("subscriber_count")) == 1, "subscriber_count() == 1（Vec<Box<dyn FnMut(&str)>> 真的持有 1 个）")
	# `subscribe` 返回的是**订阅者总数**（`subscribers.len() + callables.len()`）⇒ 1 + 1 = 2
	ok(int(btn.call("subscribe", Callable(self, "_callable_sink"))) == 2,
		"subscribe(Callable) 之后订阅者总数 == 2（1 个闭包 + 1 个 Callable）")
	ok(int(btn.call("callable_count")) == 1, "callable_count() == 1（Vec<Callable> 真的持有 1 个）")

	# --- 进树：`_ready` 才连那 3 条原生信号（机制 2）---
	root.add_child(btn)
	await process_frame
	ok(int(btn.call("ready_call_count")) >= 1, "_ready 被引擎调到（%d 次）" % int(btn.call("ready_call_count")))
	for sig_name in ["pressed", "button_up", "button_down"]:
		ok(btn.is_connected(sig_name, Callable(btn, sig_name)),
			"ready 把 %s 连到了自己的同名方法（is_connected 为真）" % sig_name)

	# --- 机制 2 + 3 + 1 一次跑通：发原生 pressed 信号 ---
	btn.emit_signal("pressed")
	ok(int(btn.call("pressed_call_count")) == 1, "emit_signal(\"pressed\") ⇒ 我们的 pressed() 被调到 1 次")
	ok(String(btn.call("native_captured")) == "X",
		"机制 3 原版形态：Box<dyn FnMut(&str)> 收到 \"X\"（实际 \"%s\"）" % btn.call("native_captured"))
	ok(_callable_hits.size() == 1 and String(_callable_hits[0]) == "X",
		"机制 3 替代方案：GDScript 的 Callable 收到 \"X\"（实际 %s）" % str(_callable_hits))
	ok(_input_selected.size() == 1 and String(_input_selected[0]) == "X",
		"机制 1：GDScript 收到自造信号 input_selected(\"X\")")
	ok(int(btn.call("input_selected_emit_count")) == 1, "input_selected_emit_count() == 1")

	# --- 长按：button_down → 下一帧 _process 派发 long_press_input_text（一次性）---
	btn.call("button_down")
	ok(bool(btn.call("press_stamp_is_pressed")), "button_down 之后状态机认为\"按着\"")
	await process_frame
	await process_frame
	ok(String(btn.call("dispatch_log")) == "X|S",
		"dispatch_log == \"X|S\"（先点一下的 input_text，后长按的 long_press_input_text；实际 \"%s\"）"
		% btn.call("dispatch_log"))
	ok(_lp_finished == 1, "机制 1：GDScript 收到长按信号 1 次（实际 %d）" % _lp_finished)
	ok(int(btn.call("long_press_emit_count")) == 1, "long_press_emit_count() == 1（一次性，只发一次）")
	ok(not bool(btn.call("press_stamp_is_pressed")), "派发之后状态机已经写回哨兵（一次性）")

	# 再等几帧也不会重复发（`0x4e42d7` 把哨兵写在派发**之前**）
	await process_frame
	await process_frame
	ok(int(btn.call("long_press_emit_count")) == 1, "再等 2 帧仍是 1 次（不重复派发）")

	# --- button_up：只改 nanos ---
	btn.call("button_down")
	btn.call("button_up")
	ok(int(btn.call("button_up_call_count")) == 1, "button_up_call_count() == 1")
	ok(not bool(btn.call("press_stamp_is_pressed")), "button_up 之后不再\"按着\"")

	# --- 长按文本为空 ⇒ `process` 第一步就返回（原版 `0x4e42bb` 的 is_empty）---
	var before: int = int(btn.call("long_press_emit_count"))
	btn.set("long_press_input_text", "")
	btn.call("button_down")
	await process_frame
	await process_frame
	ok(int(btn.call("long_press_emit_count")) == before, "long_press_input_text 为空时 process 不派发")

	# --- 没有 AnimationRect 子节点 ⇒ animation_linked() 为假，且不炸 ---
	ok(not bool(btn.call("animation_linked")), "没有 AnimationRect 子节点 ⇒ animation_linked() == false")
	var probe: String = btn.call("reentrancy_probe")
	print("      reentrancy_probe() = %s" % probe)
	ok(probe.begins_with("panic:"), "闭包捕获 Gd<Self> 再 bind_mut() 会 panic（被 catch_unwind 接住）")

	btn.call("unsubscribe_all")
	ok(int(btn.call("subscriber_count")) == 0 and int(btn.call("callable_count")) == 0,
		"unsubscribe_all() 之后两张表都空了")

	btn.queue_free()
	await process_frame

## `subscribe(Callable)` 用的接收函数：记下收到的文本。
##
## 它是**机制 3 的替代方案**那一侧的证据 —— GDScript 只能把一个 `Callable` 交进去。
func _callable_sink(text: String) -> void:
	_callable_hits.append(text)

# ===========================================================================
# B. GraphwarFunctionInput：机制 1（裸 emit_signal）+ 2 + 3（单个 dyn 回调）
# ===========================================================================
func _check_function_input() -> void:
	print("--- B. %s ---" % INPUT_CLASS)
	if not ClassDB.class_exists(INPUT_CLASS):
		return

	var inp: Object = ClassDB.instantiate(INPUT_CLASS)
	ok(inp != null and inp is LineEdit, "%s 能实例化且是 LineEdit" % INPUT_CLASS)
	if inp == null:
		return
	print("      inp.get_class() = %s" % inp.get_class())

	for method in [
		"on_text_change", "on_text_submitted", "on_size_changed", "force_update_cb",
		"expression", "simulate_key", "subscribe", "subscribe_native_probe",
	]:
		ok(_has_method(inp, method), "方法 %s() 存在" % method)

	# `#[export] bool`
	inp.set("caret_condition", true)
	ok(bool(inp.get("caret_condition")) == true, "属性 caret_condition 往返一致（true）")

	# 机制 1：注册 + 从 GDScript 连
	var sigs: PackedStringArray = _signal_names(inp)
	ok(sigs.has("expression_changed"), "自造信号 expression_changed 已注册")
	ok(sigs.has("function_submitted"), "自造信号 function_submitted 已注册")
	ok(inp.connect("expression_changed",
		func(expr: String) -> void: _expression_changed.append(expr)) == 0,
		"GDScript connect(\"expression_changed\", cb) 返回 OK")
	ok(inp.connect("function_submitted",
		func(text: String) -> void: _function_submitted.append(text)) == 0,
		"GDScript connect(\"function_submitted\", cb) 返回 OK")

	# 机制 3：单个 dyn 回调 + 一个 Callable
	ok(bool(inp.call("subscribe_native_probe")), "subscribe_native_probe() 设上了那个单 dyn 回调")
	ok(int(inp.call("subscribe", Callable(self, "_callable_sink"))) == 1, "subscribe(Callable) 返回 1")

	# 进树 → `_ready` 连三条 LineEdit 原生信号（机制 2）
	root.add_child(inp)
	await process_frame
	ok(int(inp.call("ready_call_count")) >= 1, "_ready 被引擎调到（%d 次）" % int(inp.call("ready_call_count")))
	for pair in [["text_changed", "on_text_change"], ["text_submitted", "on_text_submitted"]]:
		ok(inp.is_connected(pair[0], Callable(inp, pair[1])),
			"ready 把 %s 连到了 %s（is_connected 为真）" % [pair[0], pair[1]])
	# 第 3 条（原版字面量 `size_changed`，`.rodata 0x1c46d2`）在 Godot 4.6/4.7 上**不是信号**：
	# `connect` 返回 `ERR_INVALID_PARAMETER`。这不是本文件的笔误，是原版自己的问题。
	print("      size_changed 的连接结果: \"%s\"（wired=%d）"
		% [inp.call("connect_errors"), int(inp.call("wired_count"))])
	ok(int(inp.call("wired_count")) == 2,
		"ready 只连上 2 条（第 3 条 size_changed 在 Godot 4.6+ 上没有这条信号）")
	ok(String(inp.call("connect_errors")).begins_with("size_changed"),
		"connect_errors() 记下了 size_changed 的失败（实测证据）")
	ok(not inp.is_connected("size_changed", Callable(inp, "on_size_changed")),
		"size_changed 确实没连上（is_connected 为假）")

	# --- 预处理：只有"删空格"这一步 ---
	var edit: LineEdit = inp
	edit.text = "sin ( x )"
	inp.emit_signal("text_changed", "sin ( x )")
	ok(int(inp.call("text_change_count")) == 1, "emit_signal(\"text_changed\") ⇒ on_text_change 被调到 1 次")
	ok(String(inp.call("expression")) == "sin(x)",
		"去空格后的表达式 == \"sin(x)\"（实际 \"%s\"）" % inp.call("expression"))
	ok(_expression_changed.size() == 1 and String(_expression_changed[0]) == "sin(x)",
		"机制 1：GDScript 收到 expression_changed(\"sin(x)\")")
	ok(int(inp.call("expression_emit_count")) == 1, "expression_emit_count() == 1（值没变就不重发）")
	inp.emit_signal("text_changed", "sin ( x )")
	ok(int(inp.call("expression_emit_count")) == 1, "同一文本再发一次 text_changed ⇒ 不重复发信号")

	# 输入框**不**做 π 规范化 / 不大写转小写（那是解析器的事）
	inp.emit_signal("text_changed", "PI + π")
	ok(String(inp.call("expression")) == "PI+π",
		"PI 与 π 原样保留（π→n 在 MathFunction::new 里，不在输入框）")

	# --- `on_size_changed` 本身是好的（只是连不到那条不存在的信号上）---
	inp.call("on_size_changed")
	ok(int(inp.call("size_changed_count")) == 1, "直接调 on_size_changed() ⇒ 计数 1（方法本身没问题）")

	# --- 提交路径：文本**原样**交给回调 ---
	var hits_before: int = _callable_hits.size()
	inp.emit_signal("text_submitted", " s in ( x ) ")
	ok(int(inp.call("submit_count")) == 1, "emit_signal(\"text_submitted\") ⇒ on_text_submitted 被调到 1 次")
	ok(String(inp.call("fire_captured")) == " s in ( x ) ",
		"机制 3 原版形态：单 dyn 回调收到**带空格的原文**（实际 \"%s\"）" % inp.call("fire_captured"))
	ok(_callable_hits.size() == hits_before + 1
			and String(_callable_hits[hits_before]) == " s in ( x ) ",
		"机制 3 替代方案：GDScript 的 Callable 收到**带空格的原文**（实际 \"%s\"）"
		% str(_callable_hits[hits_before]))
	ok(_function_submitted.size() == 1 and String(_function_submitted[0]) == " s in ( x ) ",
		"机制 1：GDScript 收到 function_submitted 的原文")
	ok(String(inp.call("emit_errors")) == "", "两次自造信号 emit 都没报错（emit_errors 为空）")

	# --- 28 条折叠（`gui_input` 的判定本体）---
	# 每次先把文本清空，**只调一次** `simulate_key`（它真的会往输入框里插字）。
	edit.text = ""
	var got_a: String = inp.call("simulate_key", 0x00E2, false)
	ok(got_a == "a", "â（0xE2）折成 \"a\"（实际 \"%s\"）" % got_a)

	edit.text = ""
	var got_sup: String = inp.call("simulate_key", 0x00B2, false)
	ok(got_sup == "2", "上标 ²（0xB2）折成 \"2\"（实际 \"%s\"）" % got_sup)

	edit.text = ""
	var got_ascii: String = inp.call("simulate_key", 0x0078, false)
	ok(got_ascii == "", "ASCII 'x' 不接管（返回空串 = 交给基类）")

	edit.text = ""
	var got_caret_true: String = inp.call("simulate_key", 0x5E, true)
	ok(got_caret_true == "^", "'^' 在条件为真时插入 \"^\"（实际 \"%s\"）" % got_caret_true)

	edit.text = ""
	var got_caret_false: String = inp.call("simulate_key", 0x5E, false)
	ok(got_caret_false == "", "'^' 在条件为假时交给基类")
	ok(int(inp.call("insert_count")) == 3,
		"插字 3 次（â / ² / '^'；实际 %d）" % int(inp.call("insert_count")))

	# --- `force_update_cb` 直接调 ---
	edit.text = "sqrt ( x ) + 1"
	ok(String(inp.call("force_update_cb")) == "sqrt(x)+1",
		"force_update_cb() 读引擎文本并去掉空格（实际 \"%s\"）" % inp.call("force_update_cb"))
	ok(int(inp.call("force_update_count")) >= 1, "force_update_count() >= 1")

	# --- `_gui_input`：能不能从 GDScript 直接调虚方法 ---
	var gui_listed: bool = _has_method(inp, "_gui_input")
	print("      _gui_input 在 get_method_list() 里: %s；has_method(\"_process\") = %s"
		% [str(gui_listed), str(inp.has_method("_process"))])
	# ⚠️ 虚方法**不能**用 `Object.call("_gui_input", …)` 调：那句会
	# `SCRIPT ERROR: Invalid call. Nonexistent function '_gui_input (via call)'`
	# **并把当前函数整个中断**（实测：本章后面的断言全被跳过）。
	# 所以那个实验单独放进 `_probe_virtual_call`，只在全部检查跑完之后才试。
	#
	# 这里走真输入这条路：`Input.parse_input_event` → 主循环 → `Viewport` 派发 GUI 事件。
	var before_gui: int = int(inp.call("gui_input_call_count"))
	var key := InputEventKey.new()
	key.pressed = true
	key.unicode = 0x00EA  # ê
	edit.grab_focus()
	edit.text = ""
	Input.parse_input_event(key)
	await process_frame
	await process_frame
	var after_gui: int = int(inp.call("gui_input_call_count"))
	print("      Input.parse_input_event(ê) : gui_input_call_count %d -> %d, text=\"%s\""
		% [before_gui, after_gui, edit.text])
	ok(after_gui == before_gui + 1,
		"机制 2'：_gui_input 虚方法**被引擎真的派发了**（Input.parse_input_event ⇒ %d -> %d）"
		% [before_gui, after_gui])

	# --- 虚方法真的注册上了：证据是**引擎自己调的**（不是方法表里的字符串）---
	ok(int(inp.call("process_call_count")) >= 1,
		"_process 虚方法被引擎调到（%d 次）—— 这是虚方法注册成功的证据"
		% int(inp.call("process_call_count")))

	inp.queue_free()
	await process_frame

# ===========================================================================
# C. GraphwarSetupControl：机制 1 + 2（连**子节点**的信号）
# ===========================================================================
func _check_setup_control() -> void:
	print("--- C. %s ---" % SETUP_CLASS)
	if not ClassDB.class_exists(SETUP_CLASS):
		return

	var ctl: Object = ClassDB.instantiate(SETUP_CLASS)
	ok(ctl != null and ctl is Control, "%s 能实例化且是 Control" % SETUP_CLASS)
	if ctl == null:
		return
	print("      ctl.get_class() = %s" % ctl.get_class())

	for method in [
		"on_text_change", "on_visibility_change", "on_chat_submitted", "force_title_update",
		"cycle_axis_mode", "cycle_function_mode", "cycle_turns_mode", "cycle_time_mode",
		"start", "exit", "build_stub_children", "wired_count",
	]:
		ok(_has_method(ctl, method), "方法 %s() 存在" % method)

	var sigs: PackedStringArray = _signal_names(ctl)
	for s in ["options_changed", "start_requested", "exit_requested", "chat_message"]:
		ok(sigs.has(s), "自造信号 %s 已注册" % s)
	ok(ctl.connect("options_changed",
		func(a: int, f: int, t: int, tm: int) -> void: _options_changed.append([a, f, t, tm])) == 0,
		"GDScript connect(\"options_changed\", cb) 返回 OK（4 个 int 参数）")
	ok(ctl.connect("start_requested", func() -> void: _start_requested += 1) == 0,
		"GDScript connect(\"start_requested\", cb) 返回 OK")
	ok(ctl.connect("exit_requested", func() -> void: _exit_requested += 1) == 0,
		"GDScript connect(\"exit_requested\", cb) 返回 OK")
	ok(ctl.connect("chat_message", func(t: String) -> void: _chat_message.append(t)) == 0,
		"GDScript connect(\"chat_message\", cb) 返回 OK")

	# 替身子节点（`client/` 里没有 GameSetup 这一屏的场景）
	ok(int(ctl.call("build_stub_children")) == 3, "build_stub_children() 建了 3 个替身节点")
	var control: Control = ctl
	ok(control.get_child_count() >= 2, "至少有 2 个直接子节点（实际 %d）" % control.get_child_count())

	# 进树 → `_ready` 按名找子节点并 connect（机制 2：**别人的**信号）
	root.add_child(ctl)
	await process_frame
	ok(int(ctl.call("ready_call_count")) >= 1, "_ready 被引擎调到（%d 次）" % int(ctl.call("ready_call_count")))
	ok(int(ctl.call("wired_count")) == 3,
		"ready 连上了 3 条子节点信号（实际 %d；缺失: \"%s\"）"
		% [int(ctl.call("wired_count")), ctl.call("missing_nodes")])

	var room: Node = control.find_child("RoomNameLineEdit", true, false)
	var popup: Node = control.find_child("ChatPopUp", true, false)
	var chat: Node = control.find_child("ChatLineEdit", true, false)
	ok(room is LineEdit, "替身 RoomNameLineEdit 是 LineEdit")
	ok(popup is Control, "替身 ChatPopUp 是 Control")
	ok(chat is LineEdit, "替身 ChatLineEdit 是 LineEdit（在 ChatPopUp 底下）")
	if room != null and popup != null and chat != null:
		ok(room.is_connected("text_changed", Callable(ctl, "on_text_change")),
			"RoomNameLineEdit.text_changed → ctl.on_text_change（is_connected 为真）")
		ok(popup.is_connected("visibility_changed", Callable(ctl, "on_visibility_change")),
			"ChatPopUp.visibility_changed → ctl.on_visibility_change（is_connected 为真）")
		ok(chat.is_connected("text_submitted", Callable(ctl, "on_chat_submitted")),
			"ChatLineEdit.text_submitted → ctl.on_chat_submitted（is_connected 为真）")

		# 发子节点的原生信号（`set_text` 不会发 `text_changed`，所以由脚本充当引擎）
		room.emit_signal("text_changed", "My Room")
		ok(int(ctl.call("text_change_count")) == 1, "room.emit_signal(\"text_changed\") ⇒ on_text_change 被调到 1 次")
		ok(String(ctl.call("room_name")) == "My Room", "房间名记下来了（\"%s\"）" % ctl.call("room_name"))
		ok(String(ctl.call("title_text")) == "EveryUnit NormalFunction SequentialTurns Timer1m",
			"标题 = 四格线上名字（实际 \"%s\"）" % ctl.call("title_text"))

		popup.emit_signal("visibility_changed")
		ok(int(ctl.call("visibility_change_count")) == 1, "popup.emit_signal(\"visibility_changed\") ⇒ 收到 1 次")

		chat.emit_signal("text_submitted", "gg")
		ok(int(ctl.call("chat_submitted_count")) == 1, "chat.emit_signal(\"text_submitted\") ⇒ 收到 1 次")
		ok(String(ctl.call("chat_log")) == "gg", "聊天记录 == \"gg\"")
		ok(_chat_message.size() == 1 and String(_chat_message[0]) == "gg",
			"机制 1：GDScript 收到 chat_message(\"gg\")")

	# --- 机制 1：四格模式（4 个 int 参数）---
	ok(int(ctl.call("cycle_time_mode")) == 3, "cycle_time_mode() 返回新的轴模式判别值（3 = EveryUnit）")
	ok(_options_changed.size() == 1 and _options_changed[0] == [3, 0, 0, 2],
		"GDScript 收到 options_changed(3, 0, 0, 2)（实际 %s）" % str(_options_changed))
	ok(String(ctl.call("timer_label")) == "01:59", "时限循环到 Timer2m ⇒ 标签 01:59（实际 \"%s\"）" % ctl.call("timer_label"))
	ok(String(ctl.call("options_names")) == "EveryUnit|NormalFunction|SequentialTurns|Timer2m",
		"四格名字跟着变（实际 \"%s\"）" % ctl.call("options_names"))
	ok(String(ctl.call("options_icons")) != "", "四格图标路径非空（%s）" % ctl.call("options_icons"))

	ok(int(ctl.call("cycle_axis_mode")) == 0, "cycle_axis_mode() 3 → 0（EveryUnit 的下一个是 NoAxis）")
	ok(int(ctl.call("options_changed_count")) == 2, "options_changed 累计发了 2 次")

	# --- Start：没对手 ⇒ **纯静默**（原版观测）---
	ok(bool(ctl.call("start")) == false, "没有玩家时 start() 返回 false")
	ok(_start_requested == 0, "没有玩家时**不发** start_requested（原版是纯静默，不给提示）")

	ok(int(ctl.call("add_human", "Alice", 1, 2)) == 1, "add_human(\"Alice\", 队1, 2 兵) ⇒ 1 行")
	ok(bool(ctl.call("start")) == false, "只有一队时 start() 仍是 false")
	ok(int(ctl.call("add_bot", 3, "Deep Blue", 2, 2)) == 2, "add_bot(难度3, \"Deep Blue\", 队2, 2 兵) ⇒ 2 行")
	ok(String(ctl.call("entries_text")) == "Alice:1:2|Deep Blue:2:2:bot3",
		"条目文本（实际 \"%s\"）" % ctl.call("entries_text"))
	ok(bool(ctl.call("start")) == true, "两队都有人 ⇒ start() 返回 true")
	ok(_start_requested == 1, "机制 1：GDScript 收到 start_requested 1 次")

	ok(bool(ctl.call("add_soldier", 0)) == true, "add_soldier(0) 成功")
	ok(bool(ctl.call("add_soldier", 0)) == true, "add_soldier(0) 再成功")
	ok(bool(ctl.call("add_soldier", 0)) == true, "add_soldier(0) 第三次成功（2 → 5）")
	ok(bool(ctl.call("add_soldier", 0)) == false, "第 6 个士兵加不进去（饱和在 5）")
	ok(bool(ctl.call("change_team", 0)) == true, "change_team(0) 成功")
	ok(String(ctl.call("entries_text")).begins_with("Alice:2:5"),
		"换队 + 加兵都生效（\"%s\"）" % ctl.call("entries_text"))
	ok(bool(ctl.call("add_soldier", 99)) == false, "越界索引返回 false（不 panic）")

	ok(bool(ctl.call("exit")) == true, "exit() 返回 true")
	ok(_exit_requested == 1, "机制 1：GDScript 收到 exit_requested 1 次")

	ctl.queue_free()
	await process_frame

# ===========================================================================
# D. 机制 3 的边界：GDScript 到底能把什么交进去（引擎侧证据，不是猜测）
# ===========================================================================
func _check_mechanism3_limits() -> void:
	print("--- D. 机制 3 的边界 ---")
	if not ClassDB.class_exists(BUTTON_CLASS) or not ClassDB.class_exists(INPUT_CLASS):
		return

	var btn: Object = ClassDB.instantiate(BUTTON_CLASS)
	var inp: Object = ClassDB.instantiate(INPUT_CLASS)
	if btn == null or inp == null:
		return

	var subscribe_args: Array = _arg_types(btn, "subscribe")
	print("      GraphwarButton.subscribe 的参数类型 = %s" % str(subscribe_args))
	ok(subscribe_args.size() == 1 and String(subscribe_args[0]) == "Callable",
		"subscribe() 的参数类型是 Callable —— GDScript 交给它的只能是 Callable")

	var probe_args: Array = _arg_types(btn, "subscribe_native_probe")
	print("      GraphwarButton.subscribe_native_probe 的参数类型 = %s" % str(probe_args))
	ok(probe_args.is_empty(),
		"subscribe_native_probe() **没有参数** —— 闭包只能在 Rust 侧构造")

	var input_probe_args: Array = _arg_types(inp, "subscribe_native_probe")
	ok(input_probe_args.is_empty(),
		"GraphwarFunctionInput.subscribe_native_probe() 同样没有参数")

	# 那张表**不是**属性：Godot 侧看不见它（`#[export]` 的是 `input_text` 这类字段）
	ok(not _has_property(btn, "subscribers"),
		"Vec<Box<dyn FnMut(&str)>> **不在**属性表里（不是 #[export]，Godot 侧不可见）")
	ok(_has_property(btn, "input_text") and _has_property(btn, "long_press_input_text"),
		"两个 #[export] GString 字段在属性表里（对照）")

	# `Callable` 这个**类型**在本仓库没被当属性用过 —— 这一条只是把"没验过"说清楚：
	# gdext 对 `Callable`/`Signal`/`Rid` 只生成 `SimpleVar`，不生成 `Export`
	# （godot-core-0.5.5/src/registry/property/mod.rs 的 `no_export` 分支）。
	var has_callable_prop := false
	for p in btn.get_property_list():
		if int(p.type) == TYPE_CALLABLE:
			has_callable_prop = true
	ok(not has_callable_prop,
		"本类没有一个 Callable 类型的属性（gdext 不给 Callable 生成 Export）")

	btn.free()
	inp.free()

## 最后一个实验：从 GDScript 直接 `call()` 一个虚方法会怎样。
##
## **实测**：`SCRIPT ERROR: Invalid call. Nonexistent function '_gui_input (via call)'`，
## 而且**当前函数被整个中断**（后面的语句一条都不跑）。所以它必须：
## ① 单独一个函数；② 放在全部断言**之后**。
func _probe_virtual_call() -> void:
	print("--- E. 虚方法能不能从 GDScript call() ---")
	if not ClassDB.class_exists(INPUT_CLASS):
		return
	var inp: Object = ClassDB.instantiate(INPUT_CLASS)
	if inp == null:
		return
	var before: int = int(inp.call("gui_input_call_count"))
	var key := InputEventKey.new()
	key.pressed = true
	key.unicode = 0x00EA
	print("      has_method(\"_gui_input\") = %s；下面这一句预计会 SCRIPT ERROR 并中断本函数"
		% str(inp.has_method("_gui_input")))
	_probe_leftover = inp
	inp.call("_gui_input", key)
	# 走不到这里：上面那句会中断本函数。
	print("      （这一行不该出现）gui_input_call_count %d -> %d"
		% [before, int(inp.call("gui_input_call_count"))])

func _finish() -> void:
	if _probe_leftover != null and is_instance_valid(_probe_leftover):
		_probe_leftover.free()
	var passed: int = _checks - _failures.size()
	print("SIGNAL_CLASSES %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
