extends GwScreen
## ChatPopUp —— 聊天。
##
## 结构（`docs/92` §4.12，恢复出的 chat_pop_up.tscn **只有 7 个节点**）：
##   Control
##   └ ChatContainer/ChatContainer2
##        ├ MarginContainer/GraphwarButton（空文本，发送按钮）
##        ├ ChatLineEdit（text_submitted → on_chat_submitted，Rust 侧）
##        └ ChatBox（RichTextLabel，`ChatBox.gd`：点它 → chat_line_edit.grab_focus()）
##
## `ChatBox.gd` 是**有行为的 5 个脚本之一**（`docs/101` §4.3），行为就是"点聊天区聚焦输入框"，
## 本项目照抄（鼠标/触屏两路，用 `_gui_input` 简化成鼠标）。
##
## 消息本身走协议（`docs/24`），本项目没有网络层：发送只回显到本地 ChatBox，并注明未发送。

var _chat_box: RichTextLabel
var _line_edit: LineEdit
var _lines: Array[String] = []

func _ready() -> void:
	font_divisor = 50        # ChatContainer.gd: 宽/50
	super()

func build() -> void:
	var d := GwUI.dialog(self, Vector2(640, 360))
	var content: VBoxContainer = d.content
	content.name = "ChatContainer2"

	_chat_box = GwUI.rich()
	_chat_box.name = "ChatBox"
	_chat_box.fit_content = false
	_chat_box.scroll_active = true
	_chat_box.scroll_following = true
	_chat_box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_chat_box.custom_minimum_size = Vector2(0, 220)
	_chat_box.gui_input.connect(_on_chat_box_input)
	_chat_box.tooltip_text = "点这里会聚焦输入框（ChatBox.gd 的行为，docs/101 §4.3）"
	content.add_child(_chat_box)

	var row := GwUI.hbox("MarginContainer", 8, content)
	_line_edit = LineEdit.new()
	_line_edit.name = "ChatLineEdit"
	_line_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_line_edit.custom_minimum_size = Vector2(0, 40)
	_line_edit.placeholder_text = "message…"
	_line_edit.text_submitted.connect(_on_submit)
	row.add_child(_line_edit)

	var send := GwButton.make("      ")
	send.name = "GraphwarButton"
	send.icon = GwIcons.get_icon("new_chat")
	send.custom_minimum_size = Vector2(48, 40)
	send.tooltip_text = "发送（原版这个按钮没有 text/icon，靠主题画，docs/92 §4.12）"
	send.pressed.connect(func(): _on_submit(_line_edit.text))
	row.add_child(send)

	var actions := GwUI.hbox("HBoxContainer", 8, content)
	actions.add_child(GwUI.exit_pair(GwCommands.TOGGLE_CHAT))

func _on_chat_box_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and (event as InputEventMouseButton).pressed:
		_line_edit.grab_focus()

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
