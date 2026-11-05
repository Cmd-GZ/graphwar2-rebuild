extends GwScreen
class_name GwConnectPopup
## JoinIpPopUp / ConnectingPopUp / DisconnectedPopUp —— 三个"连接"弹窗。
##
## 结构（`docs/92` §4.7–§4.9，三个场景分别 31 / 28 / 28 节点）：
##   JoinIpPopUp:        Label("Ip:") · JoinIpIpLineEdit（空） · Cancel · Join
##   ConnectingPopUp:    Label("Connecting...") · Cancel
##   DisconnectedPopUp:  DisconnectedLabel("You have been disconnected.") · Ok
## 三者外层都有 `MarginContainer2/ExitButtonContainer`（返回图标）与底部 AdContainer。
##
## 观测（`docs/92` §4.7）：地址**必须**是 `host:port`；只写 IP 时原版毫无反应。
## `App._submit_join_ip()` 照抄了这条校验。真正的 TCP 连接/握手属于 Rust 侧
## （`docs/33`、`src/protocol.rs`），本项目没有网络层，所以：校验通过 → ConnectingPopUp
## →（1.2 s）→ DisconnectedPopUp。**这条链路是模拟的**，docs/108 §7 有写。

## popup 种类：三个界面共用这一份实现，子类只改 `kind`（三者只差两行文字）。
## `join_ip_pop_up.gd` / `connecting_pop_up.gd` / `disconnected_pop_up.gd` 都继承本类。
var kind := "join_ip"

var _edit: LineEdit

func _ready() -> void:
	font_divisor = 40        # IpPopUpMargin.gd: 宽/40
	super()

func build() -> void:
	var d := GwUI.dialog(self, Vector2(560, 240))
	var content: VBoxContainer = d.content

	var top := GwUI.hbox("HBoxContainer", 8, content)
	top.add_child(GwUI.exit_pair(_exit_command()))

	var row := GwUI.hbox("HBoxContainer", 10, content)
	match kind:
		"join_ip":
			var prompt := GwUI.label("Ip:")
			prompt.name = "Label"
			row.add_child(prompt)
			_edit = LineEdit.new()
			_edit.name = "JoinIpIpLineEdit"
			_edit.custom_minimum_size = Vector2(320, 40)
			_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			_edit.placeholder_text = "host:port"
			_edit.tooltip_text = "必须带端口（原版只写 IP 时没有反应，docs/92 §4.7）"
			row.add_child(_edit)
		"connecting":
			var l1 := GwUI.label("Connecting...")
			l1.name = "Label"
			l1.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			l1.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
			row.add_child(l1)
		"disconnected":
			var l2 := GwUI.label("You have been disconnected.")
			l2.name = "DisconnectedLabel"
			l2.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			l2.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
			row.add_child(l2)

	var actions := GwUI.hbox("HBoxContainer3", 10, content)
	var spacer := GwUI.spacer(Vector2(0, 0))
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(spacer)
	match kind:
		"join_ip":
			var cancel := GwButton.make(" Cancel ", GwCommands.CANCEL_JOIN_IP)
			cancel.custom_minimum_size = Vector2(120, 42)
			actions.add_child(cancel)
			var join := GwButton.make("  Join  ", GwCommands.JOIN_JOIN_IP)
			join.custom_minimum_size = Vector2(120, 42)
			actions.add_child(join)
			_edit.text_submitted.connect(func(_t): get_node("/root/App").command(GwCommands.JOIN_JOIN_IP))
		"connecting":
			var c2 := GwButton.make(" Cancel ", GwCommands.CANCEL_CONNECTING)
			c2.custom_minimum_size = Vector2(120, 42)
			actions.add_child(c2)
		"disconnected":
			var ok := GwButton.make(" Ok ", GwCommands.OK_DISCONNECTED)
			ok.custom_minimum_size = Vector2(120, 42)
			actions.add_child(ok)

	(d.vbox as VBoxContainer).add_child(GwUI.ad_bar())

func _exit_command() -> String:
	match kind:
		"join_ip": return GwCommands.CANCEL_JOIN_IP
		"connecting": return GwCommands.CANCEL_CONNECTING
		_: return GwCommands.OK_DISCONNECTED

func current_text() -> String:
	return _edit.text if _edit != null else ""

func on_enter() -> void:
	if _edit != null:
		_edit.grab_focus()
