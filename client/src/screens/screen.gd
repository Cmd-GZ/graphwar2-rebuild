extends Control
class_name GwScreen
## 界面基类。
##
## 每个界面：一个 `scenes/screens/*.tscn`（根 Control + 本脚本的子类）+ 一个 `build()`。
## 子类只实现 `build()`（搭控件）和需要时的 `on_enter()`（进入时刷新数据）。
##
## 字号缩放：原版有 32 个脚本连 `get_tree().root.size_changed` 重算字号
## （`docs/104` §6.2），规则是"字号 = 屏幕宽 ÷ N"（`docs/101` §5.2）。
## 这里用 `apply_font_scale()` 做同一件事，N 由各界面给出（见各屏 `FONT_DIVISOR`）。

## 该界面的字号除数 N（屏幕宽 ÷ N）。0 = 用主题默认字号。
var font_divisor: int = 30

## 是否用安全区宽度而不是 viewport 宽度（观测：只有 `PlayerTabContainer` 用安全区，
## 见 `docs/101` §5.2 / §4.4）。
var use_safe_area := false

var _built := false

func _ready() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)
	visible = false
	build()
	_built = true
	_install_font_scaling()

func build() -> void:
	pass

func on_enter() -> void:
	pass

func app() -> Node:
	return get_node_or_null("/root/App")

func notify(text: String) -> void:
	var a := app()
	if a != null:
		a.call("notify", text)

# ---------------------------------------------------------------- 字号缩放

func _install_font_scaling() -> void:
	var root := get_tree().root
	if root != null and not root.size_changed.is_connected(_on_viewport_resized):
		root.size_changed.connect(_on_viewport_resized)
	apply_font_scale()

func _on_viewport_resized() -> void:
	apply_font_scale()

## 按 docs/101 §5.2 的规则算当前字号：宽度 ÷ font_divisor。
func current_font_size() -> int:
	if font_divisor <= 0:
		return GwThemeFactory.BASE_FONT_SIZE
	var width: float
	if use_safe_area:
		width = float(DisplayServer.get_display_safe_area().size.x)
		if width <= 0.0:
			width = get_viewport_rect().size.x
	else:
		width = get_viewport_rect().size.x
	if width <= 0.0:
		return GwThemeFactory.BASE_FONT_SIZE
	return maxi(10, int(width / float(font_divisor)))

## 把字号写到本界面所有"会显示文字"的控件上（等价于原版 32 个 `add_theme_font_size_override`）。
func apply_font_scale() -> void:
	if not _built:
		return
	var size := current_font_size()
	_apply_size_recursive(self, size)

func _apply_size_recursive(node: Node, size: int) -> void:
	if node is Label or node is Button or node is LineEdit:
		(node as Control).add_theme_font_size_override("font_size", size)
	elif node is RichTextLabel:
		(node as RichTextLabel).add_theme_font_size_override("normal_font_size", maxi(10, int(size * 0.85)))
	for c in node.get_children():
		_apply_size_recursive(c, size)

# ---------------------------------------------------------------- 小工具

## 顶部"标题 + 返回按钮"行，几乎所有原版界面都有这个形状。
## 返回 {row, title_label}
func build_header(parent: Node, title_text: String, exit_cmd: String) -> Dictionary:
	var row := GwUI.hbox("HBoxContainer", 8, parent)
	var exit := GwUI.exit_pair(exit_cmd)
	exit.name = "ExitButtonContainer"
	row.add_child(exit)
	var m := GwUI.margin("MarginContainer", 4, row)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var lbl := GwUI.title(title_text, m)
	return {"row": row, "title": lbl, "exit": exit}
