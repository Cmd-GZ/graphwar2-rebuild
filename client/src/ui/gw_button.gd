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
	if icon_ != null:
		b.icon = icon_
		b.add_theme_constant_override("h_separation", 6)
	return b
