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
	# **节点树来自原版 `recovered-src/chat_pop_up.tscn`（逐字节装进 `client/scenes/screens/`）**，
	# 这里只做接线：取两个句柄 + 把 `text_submitted` 接上。
	# 原版这层 `Control` / `ChatContainer` / `ChatContainer2` 的 `mouse_filter` 全是
	# **2（IGNORE）** —— 那条写在 .tscn 里，不再由代码设。
	var content: Node = get_node("ChatContainer/ChatContainer2")
	_line_edit = content.get_node("ChatLineEdit") as LineEdit
	_chat_box = content.get_node("ChatBox") as RichTextLabel
	_line_edit.text_submitted.connect(_on_submit)


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
