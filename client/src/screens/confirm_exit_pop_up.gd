extends GwScreen
## ConfirmPlayExitPopUp —— 退出对局确认。
##
## 结构（`docs/92` §4.11，恢复出的 confirm_exit_pop_up.tscn 31 节点）：
##   Label("Are you sure you want to exit?") · No(cancel_exit_play) · Yes(exit_play) · 返回图标
##
## 观测细节（`docs/92` §4.11）：对局里 `ExitButtonContainer/ClipButton` 单击**不弹**框，
## `LongPressButton` 单击**弹**框 —— 因为 ClipButton 只是动画层。本项目 `ui.gd::exit_pair`
## 照抄了这个分工（ClipButton 的 mouse_filter = IGNORE）。

func _ready() -> void:
	font_divisor = 40
	super()

func build() -> void:
	var d := GwUI.dialog(self, Vector2(560, 220))
	var content: VBoxContainer = d.content

	var label := GwUI.label("Are you sure you want to exit?", 0, HORIZONTAL_ALIGNMENT_CENTER)
	label.name = "Label"
	label.size_flags_vertical = Control.SIZE_EXPAND_FILL
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	content.add_child(label)

	var actions := GwUI.hbox("HBoxContainer3", 10, content)
	var spacer := GwUI.spacer(Vector2(0, 0))
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(spacer)
	var no := GwButton.make("  No  ", GwCommands.CANCEL_EXIT_PLAY)
	no.custom_minimum_size = Vector2(120, 42)
	actions.add_child(no)
	var yes := GwButton.make("  Yes  ", GwCommands.EXIT_PLAY)
	yes.custom_minimum_size = Vector2(120, 42)
	actions.add_child(yes)

	var top := GwUI.hbox("HBoxContainer", 8, content)
	top.add_child(GwUI.exit_pair(GwCommands.CANCEL_EXIT_PLAY))

	(d.vbox as VBoxContainer).add_child(GwUI.ad_bar())
