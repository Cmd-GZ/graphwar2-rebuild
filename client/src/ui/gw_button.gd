extends Button
class_name GwButton
## 复刻原版 `GraphwarButton` 的**契约**（不是它的实现；实现是 Rust 的封闭代码）。
##
## 契约来源（观测）：
##   * `docs/92` §3：原版 283 个按钮全是 Rust 类 `GraphwarButton`，无原生 `Button`；
##   * `docs/104` / 恢复出的 `.tscn`：每个按钮带 `input_text`（少数带
##     `long_press_input_text`），值就是 Rust 侧的命令键（`campaign` / `add_bot_3` / `fire`…）；
##   * `docs/92` §2.3：按钮上没有任何 GDScript 处理器，按下后的行为全在 Rust。
##
## 所以本类的全部职责是：**把 `input_text` 送给 App 的命令总线**。
## 命令键清单见 `src/app/commands.gd`。

## 按下后派发的命令键。空串表示"这个按钮不派发命令"（原版里也有：FireContainer 的
## ClipButton、GhostButton 等只是动画/占位层）。
@export var input_text: String = ""

## 长按派发的命令键（原版 `FireButton` 的 `long_press_input_text = "fire"`）。
@export var long_press_input_text: String = ""

## 长按阈值（秒）。原版阈值未观测 → 这是我们自己定的可用性参数。
@export var long_press_seconds: float = 0.45

## 首次触发后，按住不放时每隔这么久重复一次派发（0 = 不重复）。
@export var repeat_seconds: float = 0.18

var _held := 0.0
var _repeating := false

func _ready() -> void:
	focus_mode = Control.FOCUS_NONE
	if not pressed.is_connected(_on_pressed):
		pressed.connect(_on_pressed)
	if not button_down.is_connected(_on_down):
		button_down.connect(_on_down)
	if not button_up.is_connected(_on_up):
		button_up.connect(_on_up)

func _process(delta: float) -> void:
	if _held <= 0.0:
		return
	_held += delta
	# 长按：短命令只在超过阈值时触发一次，之后按 repeat_seconds 重复。
	if long_press_input_text != "" and _held >= long_press_seconds and repeat_seconds > 0.0:
		if not _repeating:
			_repeating = true
			_fire(long_press_input_text)
			_held = 0.0
			return
	if _repeating and repeat_seconds > 0.0 and _held >= repeat_seconds:
		_held = 0.0
		_fire(long_press_input_text)

func _on_pressed() -> void:
	# 只声明了 long_press 的按钮（FireButton）在单击时也派发一次：原版就是"点一下开火"。
	if input_text != "":
		_fire(input_text)
	elif long_press_input_text != "" and not _repeating:
		_fire(long_press_input_text)

func _on_down() -> void:
	_held = 0.001
	_repeating = false
	set_process(true)

func _on_up() -> void:
	_held = 0.0
	_repeating = false
	set_process(false)

func _fire(cmd: String) -> void:
	if cmd == "":
		return
	var app := get_node_or_null("/root/App")
	if app == null:
		# 自检/孤立场景里也能工作：不炸，只报一次。
		push_warning("[gw2] App autoload 不存在，命令 %s 被丢弃" % cmd)
		return
	app.command(cmd)

## 便捷构造（本项目的 UI 是代码生成的，见 docs/108 §3）。
static func make(text_: String, cmd: String = "", icon_: Texture2D = null) -> GwButton:
	var b := GwButton.new()
	b.text = text_
	b.input_text = cmd
	# `expand_icon = true`：图标按按钮大小缩放，**不参与最小尺寸**。
	# 原版每个带图标按钮都写着 expand_icon = true（如 campaign.tscn 的 ClipButton）；
	# 我们以前用 48×48 的现画图标，不写也看不出来；现在图标是原版 svg 的 512×512 导入
	# 产物，不写就会把按钮的最小尺寸顶到 512 px，整个界面被挤塌（docs/121 §7.2 实测）。
	b.expand_icon = true
	if icon_ != null:
		b.icon = icon_
		b.add_theme_constant_override("h_separation", 6)
	return b

# ---------------------------------------------------------------- 原生 `GraphwarButton` 的互操作
#
# 原版 `.tscn` 里的按钮是 **GDExtension 原生类 `GraphwarButton`**
# （`src/_scaffold/graphwar_button_class.rs`：`#[class(base = Button)]`），
# **不是**本文件这个 `extends Button` 的仿制类。原版 `.tscn` 一装进 `client/`，
# 那一屏的按钮就全是原生类 ⇒ 它们**不会**走本文件的 `_fire()`。
#
# 原版里"按钮 → 行为"的接线在 Rust 控制器里（`GraphwarButton::subscribe` 那张闭包表 +
# `input_selected` 信号）；我方客户端的命令总线是 `App.command`（`client/src/app/app.gd`）。
# 所以这里补一层：把原生按钮的两个出口接到命令总线 ——
#   * `input_selected(text)`  ← 对应原版 `pressed`（`0x4ef560`）派发的命令键（`input_text`）
#   * `long_press_finished`   ← 对应原版 `process`（`0x4e42a0`）的长按派发，命令键是
#                                `long_press_input_text`（`FireButton` = "fire"）
#
# 证据档：**观测**（结构）/ **推断**（接线点）。`input_selected` 与 `long_press_finished`
# 是本项目 Rust 类里**自造的两条信号**（原版 6 个方法里一个 `emit_signal` 都没有，见
# `graphwar_button_class.rs` 文件头"机制 1"）—— 它们存在的唯一理由就是让 GDScript 能观察到
# 派发，所以拿它们当接线点是那个设计意图的直接用法，不是猜。**原版真正的订阅者（Rust 控制器
# 里那张闭包表）我方还没复原完**，这一步是"把原生按钮接到我方命令总线"的临时桥，
# 等控制器迁回 Rust（AGENTS §0.4）之后应当由 Rust 侧订阅取代（docs/06 §92.6）。

## 一棵子树里的原生 `GraphwarButton`（深度优先前序；不含仿制类 `GwButton`）。
static func native_buttons(root: Node) -> Array:
	var out: Array = []
	_collect_native_buttons(root, out)
	return out

static func _collect_native_buttons(n: Node, out: Array) -> void:
	if n == null:
		return
	for c in n.get_children():
		if c.get_class() == "GraphwarButton":
			out.append(c)
		_collect_native_buttons(c, out)


## 把 `root` 子树里所有原生按钮接到 `App.command`（幂等：打过 `gw2_command_bound` 标记的跳过）。
## 返回本次新接上的按钮个数。
static func bind_native(root: Node) -> int:
	if root == null:
		return 0
	var bound := 0
	for b in native_buttons(root):
		if b.has_meta("gw2_command_bound"):
			continue
		b.set_meta("gw2_command_bound", true)
		# 单击/普通键：原版 `pressed` → `input_selected(input_text)`。
		b.connect("input_selected", func(text: String) -> void:
			var app: Node = b.get_node_or_null("/root/App")
			if app != null and app.has_method("command"):
				app.command(text))
		# 长按键（`FireButton`）：原版 `process` 到 500 ms 后派发 `long_press_input_text`。
		b.connect("long_press_finished", func() -> void:
			var app: Node = b.get_node_or_null("/root/App")
			if app != null and app.has_method("command"):
				app.command(String(b.get("long_press_input_text"))))
		bound += 1
	return bound
