extends GwScreen
## ChatPopUp —— 聊天。
##
## 本轮（docs/130）照恢复出的 `chat_pop_up.tscn` **逐节点**重写（那个场景只有 7 个节点），
## 并用"原版 pck 直接实例化 + 同版本引擎"的逐像素真值核对。1920×1160 下量到的矩形：
##
##   ChatContainer   (0,0)   1920×1160   ← 左右各 10 px 内边距（`ChatContainer` 内联脚本）
##   ChatContainer2  (10,0)  1900×1160
##     MarginContainer (10,0) 1900×**96**  → GraphwarButton（**看不见**的发送键，invisible_theme）
##     ChatLineEdit    (10,100) 1900×**62** ← 黑底 62.7% + 3 px 黑边 + 圆角 10
##     ChatBox         (10,166) 1900×**994** ← 黑底 62.7%、**无边框**、无圆角
##
## 也就是说：**聊天是一整屏的浮层**，不是我们以前那个 640×360 的小对话框 ——
## 上面一条看不见的发送键、中间一条输入行、下面 994 px 的聊天记录。真帧里能看见的
## 只有两块半透明黑（输入行与聊天区）。
##
## 字号：`ChatContainer` 内联脚本 = 宽/50；发送键那层是 `ExitButtonContainer.gd` = 宽/30。
## 颜色：输入行与聊天区的黑是 `StyleBoxFlat(bg = Color(0,0,0,0.627451))`，压在原版
## 深绿底上正好是 (5,14,10) —— 真帧逐像素核对过。
##
## `ChatBox.gd` 是**有行为的 5 个脚本之一**（`docs/101` §4.3），行为就是"点聊天区聚焦输入框"，
## 本项目照抄（鼠标/触屏两路，用 `_gui_input` 简化成鼠标）。
##
## 消息本身走协议（`docs/24`），本项目没有网络层：发送只回显到本地 ChatBox，并注明未发送。

## 原版 `chat_pop_up.tscn` 里两块黑的颜色（逐字照抄 StyleBoxFlat 的 bg_color）
const BLACK_BG := Color(0, 0, 0, 0.627451)

var _chat_box: RichTextLabel
var _line_edit: LineEdit
var _lines: Array[String] = []

func _ready() -> void:
	font_divisor = 50        # ChatContainer.gd：宽/50
	super()

func build() -> void:
	# 原版这层 `Control` / `ChatContainer` / `ChatContainer2` 的 `mouse_filter` 全是
	# **2（IGNORE）**：聊天浮层除了输入行和聊天区之外**不吃**鼠标 —— 否则顶部那条
	# 看不见的发送键区域会把"再点一次聊天开关"的点击吞掉，聊天就关不掉了。
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	var shell := GwUI.margins("ChatContainer", 10, 0, 10, 0, self)
	shell.set_anchors_preset(Control.PRESET_FULL_RECT)
	shell.size_flags_vertical = Control.SIZE_EXPAND_FILL
	shell.mouse_filter = Control.MOUSE_FILTER_IGNORE

	var content := GwUI.vbox("ChatContainer2", GwUI.SEPARATION)
	content.mouse_filter = Control.MOUSE_FILTER_IGNORE

	# ① 看不见的发送键：MarginContainer(ExitButtonContainer.gd, 宽/30) → GraphwarButton
	var send_holder := GwUI.margin("MarginContainer", 0)
	var send := GwButton.make("      ")      # 原版文本就是 6 个空格
	send.name = "GraphwarButton"
	send.text = "      "
	_invisible_theme(send)
	# 推断：原版这个按钮没有 `input_text`（点它本来就什么都不发生），但它 mouse_filter 是
	# STOP，会挡住底下"聊天开关"的点击。为了不把开关锁死，这里让它不接收鼠标。
	send.mouse_filter = Control.MOUSE_FILTER_IGNORE
	send.tooltip_text = "发送（原版这个按钮是 invisible_theme，看不见也点不动）"
	send.pressed.connect(func(): _on_submit(_line_edit.text))
	send_holder.add_child(send)
	# 原版 `chat_pop_up.tscn` 的 `ChatContainer/ChatContainer2/MarginContainer` 挂
	# `ExitButtonContainer.gd`（字号 = 宽/30）
	GwViews.attach(send_holder, "ExitButtonContainer.gd")
	content.add_child(send_holder)

	# ② 输入行：黑底 + 3 px 黑边 + 圆角 10
	_line_edit = LineEdit.new()
	_line_edit.name = "ChatLineEdit"
	_line_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_line_edit.add_theme_stylebox_override("normal", GwThemeFactory._flat(BLACK_BG))
	_line_edit.add_theme_stylebox_override("focus",
		GwThemeFactory._flat(Color(0.6, 0.6, 0.6, 0)))   # 原版 focus 是透明细边
	_line_edit.text_submitted.connect(_on_submit)
	content.add_child(_line_edit)

	# ③ 聊天记录：黑底 62.7%、无边框、无圆角
	_chat_box = GwUI.rich()
	_chat_box.name = "ChatBox"
	_chat_box.fit_content = false
	_chat_box.scroll_active = true
	_chat_box.scroll_following = true
	_chat_box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var box_style := StyleBoxFlat.new()
	box_style.bg_color = BLACK_BG
	_chat_box.add_theme_stylebox_override("normal", box_style)
	_chat_box.tooltip_text = "点这里会聚焦输入框（ChatBox.gd 的行为，docs/101 §4.3）"
	content.add_child(_chat_box)
	# 原版 `ChatBox.gd` 就是"点一下把焦点交给 `../ChatLineEdit`" —— 我们不再自己连 `gui_input`
	GwViews.attach(_chat_box, "ChatBox.gd")
	# 三个子节点齐了再挂 `ChatContainer.gd`（字号 = 宽/50），最后进树
	GwViews.attach(content, "ChatContainer.gd")
	shell.add_child(content)

## 原版 `invisible_theme.tres`：五态 stylebox 都指向 `invisible_button.tres`
## —— **透明底 + 3 px 透明边框 + 圆角 10**，字色 alpha 0。
## 注意不能用 `StyleBoxEmpty`：`invisible_button.tres` 的 3 px 边框会给按钮
## 6 px 的 content margin，正是这条决定了发送键的 96 px 高（⇒ 输入行落在 y=100）。
func _invisible_theme(b: Button) -> void:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0, 0, 0, 0)
	sb.border_color = Color(0, 0, 0, 0)
	sb.set_border_width_all(3)
	sb.set_corner_radius_all(10)
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		b.add_theme_stylebox_override(state, sb)
	for c in ["font_color", "font_pressed_color", "font_hover_color", "font_focus_color",
			"font_hover_pressed_color", "icon_normal_color"]:
		b.add_theme_color_override(c, Color(0, 0, 0, 0))

func _on_submit(text_: String) -> void:
	var msg := text_.strip_edges()
	if msg.is_empty():
		return
	_lines.append("%s: %s" % [_player_name(), msg])
	_line_edit.text = ""
	_refresh()
	notify("聊天没有网络层：消息只显示在本地（协议见 docs/24）")

func _player_name() -> String:
	var app_node := app()
	if app_node != null:
		return String(app_node.store.profile.name)
	return "user"

func _refresh() -> void:
	if _chat_box == null:
		return
	var text_ := ""
	for l in _lines:
		text_ += "%s\n" % l
	_chat_box.clear()
	_chat_box.append_text(text_)

func on_enter() -> void:
	_refresh()
	if _line_edit != null:
		_line_edit.grab_focus()
