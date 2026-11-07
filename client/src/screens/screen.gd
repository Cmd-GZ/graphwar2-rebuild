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
##
## ⚠️ 原版**不是每屏一个除数**：同一屏里不同容器挂的是不同的脚本、用不同的 N
## （`docs/101` §5.2 的 28 处）。例如 LanLobby 的列表头是 `LanLobbyFontResize.gd` = 宽/40，
## 而底部 Create/Join 是 `LobbyButtonContainer.gd` = 宽/30；每个界面的返回键是
## `ExitButtonContainer.gd` = 宽/30。
## 所以这里支持**按容器覆盖**：给某个节点打 `meta("font_divisor", N)`，它和它的子树
## 就改用 宽/N。没打标记的沿用本屏的 `font_divisor`。
func apply_font_scale() -> void:
	if not _built:
		return
	var width := get_viewport_rect().size.x
	if width <= 0.0:
		width = 0.0
	_apply_size_recursive(self, current_font_size(), width)

func _apply_size_recursive(node: Node, size: int, width: float) -> void:
	# 原版视图脚本（`client/src/views/*.gd`）驱动的子树**整棵跳过**：字号由原版自己算。
	# 原版 105 个脚本里 `maxi`/`max(` 出现 0 次 ⇒ 没有下限，我们那套 `maxi(10, 宽/N)` 是自加的
	# （`docs/16` §39.6）。两边都写就会变成"谁后连 size_changed 谁赢"，等于没换。
	if GwViews.owns(node):
		return
	var local_size := size
	var div := int(node.get_meta("font_divisor", 0)) if node.has_meta("font_divisor") else 0
	if div > 0 and width > 0.0:
		local_size = maxi(10, int(width / float(div)))
	if node is Label or node is Button or node is LineEdit or node is TabContainer:
		(node as Control).add_theme_font_size_override("font_size", local_size)
	elif node is RichTextLabel:
		(node as RichTextLabel).add_theme_font_size_override("normal_font_size", maxi(10, int(local_size * 0.85)))
	for c in node.get_children():
		_apply_size_recursive(c, local_size, width)

# ---------------------------------------------------------------- 小工具

## 顶部"标题 + 返回按钮"行，几乎所有原版界面都有这个形状：
##   HBoxContainer/{ExitButtonContainer, MarginContainer/Label}
## 原版这个 HBox 没有 separation 覆盖 ⇒ 4；外层 MarginContainer 的 margin 也来自安全区 = 0。
## `use_title_font`：原版有 6 个界面的标题 Label 挂 `title_theme.tres`（EraserRegular 手写体）
## —— campaign / GameSetup / player_screen / replay_lobby / endgame_pop_up / main_menu；
## 其余界面标题走 `graphwar_theme.tres`（Comic Relief）。所以字体要显式传，不能一刀切。
## 返回 {row, title_label}
func build_header(parent: Node, title_text: String, exit_cmd: String,
		use_title_font: bool = false) -> Dictionary:
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, parent)
	var exit := GwUI.exit_pair(exit_cmd)
	exit.name = "ExitButtonContainer"
	row.add_child(exit)
	var m := GwUI.margin("MarginContainer", 0)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var lbl := GwUI.title(title_text, m, use_title_font)
	# 标题外层挂**原版** `LanLobbyTitle.gd`（字号 = 宽/30）：campaign / player_screen /
	# replay_lobby 三个 `.tscn` 的标题容器都是它（节点路径见 `docs/16` §40）。
	GwViews.attach(m, "LanLobbyTitle.gd")
	row.add_child(m)
	return {"row": row, "title": lbl, "exit": exit}
