extends GwScreen
class_name GwConnectPopup
## JoinIpPopUp / ConnectingPopUp / DisconnectedPopUp —— 三个"连接"弹窗。
##
## 三者共用一份实现（原版三个 `.tscn` 结构逐节点相同，只差文案与按钮个数）。
## 本轮（docs/130）照 `.tscn` 逐节点重写，用"原版 pck 直接实例化 + 同版本引擎"
## 的逐像素真值核对。1920×1160 下量到的矩形（绝对像素）：
##
##   面板外框 (243,386) 1434×383   ← 三列比例 **1:6:1**（`stretch_ratio = 6.0`）
##   文案行   y 481..576（面板内 95 高）
##   Cancel   (853,676) 194×73     ← 面板中段、按钮行居中
##   返回键   (0,0) 146×96        ← 根节点上的浮层
##
## 细节（都是原版 `.tscn` 的原样，不是我们挑的）：
##   * 文案与按钮外面都套一层 `IpPopUpMargin.gd` 的 `MarginContainer`（**四边各 10 px**，
##     字号 = 宽/40）；
##   * 按钮行两侧各一个 `[h=3,v=3]` 弹性空格，两个按钮之间**没有**额外空格；
##   * `ConnectingPopUp` / `DisconnectedPopUp` 在 Cancel/Ok 右边还有一个**空**的
##     `IpPopUpMargin` 壳（20 px 宽）—— 它让按钮组比正中偏左 10 px，原版如此，照抄；
##   * `JoinIpPopUp` 的 "Ip:" 那个 MarginContainer **不参与弹性**（没有 `size_flags`），
##     右边的 LineEdit 外壳才是 `[h=3]`。
##
## 观测（`docs/92` §4.7）：地址**必须**是 `host:port`；只写 IP 时原版毫无反应。
## `App._submit_join_ip()` 照抄了这条校验。真正的 TCP 连接/握手属于 Rust 侧
## （`docs/33`、`src/protocol.rs`），本项目没有网络层，所以：校验通过 → ConnectingPopUp
## →（1.2 s）→ DisconnectedPopUp。**这条链路是模拟的**，docs/108 §7 有写。

## popup 种类：三个界面共用这一份实现，子类只改 `kind`（三者只差两行文字）。
var kind := "join_ip"

var _edit: LineEdit

func _ready() -> void:
	font_divisor = 40        # IpPopUpMargin.gd：宽/40
	super()

func build() -> void:
	var frame := GwUI.frame(self)
	var bands: VBoxContainer = frame.vbox
	_popup_band(bands, "HBoxContainer")
	var panel_band := _popup_band(bands, "HBoxContainer2")
	_popup_band(bands, "HBoxContainer3")
	bands.add_child(GwUI.ad_bar())

	# 三列比例 1 : 6 : 1（原版 `MarginContainer2` 的 `size_flags_stretch_ratio = 6.0`）
	_expander(panel_band, "MarginContainer", 1.0)
	var panel_col := _expander(panel_band, "MarginContainer2", 6.0)
	_expander(panel_band, "MarginContainer3", 1.0)

	var panel := Panel.new()
	panel.name = "Panel"
	panel_col.add_child(panel)
	var content := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, panel_col)

	# 面板内部：上空白 [v=3] / 文案行 / 中空白 [v=3] / 按钮行 —— **没有**下空白
	_grow_spacer(content, "MarginContainer2", 1.0)
	_build_text_row(content)
	_grow_spacer(content, "MarginContainer", 1.0)
	_build_button_row(content)

	var overlay := GwUI.margin("MarginContainer2", 0, self)
	overlay.add_child(GwUI.exit_pair(_exit_command()))

## 文案行。原版三种各一行，结构不同（见文件头）。
func _build_text_row(content: VBoxContainer) -> void:
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, content)
	match kind:
		"join_ip":
			var prompt_box := _ip_margin(row, "MarginContainer", false)
			var prompt := GwUI.label("Ip:")
			prompt.name = "Label"
			prompt_box.add_child(prompt)
			var edit_box := _ip_margin(row, "MarginContainer2", true)
			_edit = LineEdit.new()
			_edit.name = "JoinIpIpLineEdit"
			_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			edit_box.add_child(_edit)
			_edit.text_submitted.connect(
				func(_t: String): get_node("/root/App").command(GwCommands.JOIN_JOIN_IP))
		"connecting":
			var box := _ip_margin(row, "MarginContainer", true)
			var l := GwUI.label("Connecting...", 0, HORIZONTAL_ALIGNMENT_CENTER)
			l.name = "Label"
			box.add_child(l)
		_:
			var box2 := _ip_margin(row, "MarginContainer", true)
			var l2 := GwUI.label("You have been disconnected.", 0, HORIZONTAL_ALIGNMENT_CENTER)
			l2.name = "DisconnectedLabel"
			box2.add_child(l2)

## 按钮行。左右各一个 `[h=3,v=3]` 弹性空格；`Connecting`/`Disconnected` 右边那个空壳
## `IpPopUpMargin` 是原版就有的（20 px），它让按钮组比正中偏左 10 px —— 照抄。
func _build_button_row(content: VBoxContainer) -> void:
	var buttons := GwUI.hbox("HBoxContainer3", GwUI.SEPARATION, content)
	_grow_spacer(buttons, "MarginContainer3", 1.0)
	match kind:
		"join_ip":
			_ip_margin(buttons, "MarginContainer", false).add_child(
				GwButton.make(" Cancel ", GwCommands.CANCEL_JOIN_IP))
			_ip_margin(buttons, "MarginContainer2", false).add_child(
				GwButton.make("  Join  ", GwCommands.JOIN_JOIN_IP))
		"connecting":
			_ip_margin(buttons, "MarginContainer", false).add_child(
				GwButton.make(" Cancel ", GwCommands.CANCEL_CONNECTING))
			_ip_margin(buttons, "MarginContainer2", false)
		_:
			_ip_margin(buttons, "MarginContainer", false).add_child(
				GwButton.make(" Ok ", GwCommands.OK_DISCONNECTED))
			_ip_margin(buttons, "MarginContainer2", false)
	_grow_spacer(buttons, "MarginContainer4", 1.0)

func _exit_command() -> String:
	match kind:
		"join_ip": return GwCommands.CANCEL_JOIN_IP
		"connecting": return GwCommands.CANCEL_CONNECTING
		_: return GwCommands.OK_DISCONNECTED

# ---------------------------------------------------------------- 原版骨架的小工具

## 原版每条竖带都是 `HBoxContainer` + `size_flags_vertical = 3`。
func _popup_band(parent: VBoxContainer, node_name: String) -> HBoxContainer:
	var b := GwUI.hbox(node_name, GwUI.SEPARATION, parent)
	b.size_flags_vertical = Control.SIZE_EXPAND_FILL
	return b

## 只按**宽度**分比例的空 `MarginContainer`（原版左右留白，竖直是 FILL）。
func _expander(parent: HBoxContainer, node_name: String, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	return m

## 两个方向都 EXPAND 的空 `MarginContainer`（原版的 `[h=3,v=3]` 弹性空格）。
func _grow_spacer(parent: Container, node_name: String, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	m.size_flags_vertical = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	return m

## `IpPopUpMargin.gd` 包一层：margin 全 10。`expand` = 原版有没有写 `size_flags_horizontal = 3`。
func _ip_margin(parent: HBoxContainer, node_name: String, expand: bool) -> MarginContainer:
	var m := GwUI.margin(node_name, 10, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL if expand else Control.SIZE_FILL
	return m

# ---------------------------------------------------------------- 状态

func current_text() -> String:
	return _edit.text if _edit != null else ""

func on_enter() -> void:
	if _edit != null:
		_edit.grab_focus()
