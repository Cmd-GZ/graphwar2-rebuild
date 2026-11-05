extends GwScreen
## JoinLevelPopUp —— 关卡预览 / 开局确认。
##
## 结构（`docs/92` §4.3，恢复出的 join_level_pop_up.tscn 42 节点）：
##   MarginContainer/VBoxContainer
##     ├ MarginContainer2/HBoxContainer/ExitButtonContainer/…（input_text="cancel_level"）
##     ├ HBoxContainer2/MarginContainer2/Panel/VBoxContainer
##     │     ├ LevelTitleLabel("Level 0")
##     │     ├ HBoxContainer/…/ScrollContainer/LevelDescriptionEdit（TextEdit，只读）
##     │     ├ OptionsContainer/AspectRatioContainer{1..4}/{Axis,Function,Turn,Time}ModeButton
##     │     └ HBoxContainer3/…/GraphwarButton(" Cancel ") + GraphwarButton("  Start  ")
##     └ AdContainer(0,60)
##
## 注意：这里的 TimeModeButton 在 GameSetup 里叫 TurnTimeButton（原版命名不一致，
## `docs/92` §4.3 已注明）。
##
## 数据来源 `campaign.gd`：**19/21 关的内容是未知的**，说明栏里写的就是这件事。

var _title: Label
var _description: TextEdit
var _mode_buttons := {}

func _ready() -> void:
	font_divisor = 30        # LevelTitleLabel.gd / LevelDescriptionScrollContainer 用 40/30
	super()

func build() -> void:
	var d := GwUI.dialog(self, Vector2(720, 420))
	var content: VBoxContainer = d.content

	var top := GwUI.hbox("HBoxContainerTop", 8, content)
	top.add_child(GwUI.exit_pair(GwCommands.CANCEL_LEVEL))
	var top_spacer := GwUI.spacer(Vector2(0, 0))
	top_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top.add_child(top_spacer)

	var head := GwUI.hbox("HBoxContainer", 8, content)
	_title = GwUI.label("Level 0", 0, HORIZONTAL_ALIGNMENT_LEFT)
	_title.name = "LevelTitleLabel"
	_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(_title)

	var scroll := GwUI.scroll("ScrollContainer")
	scroll.custom_minimum_size = Vector2(0, 160)
	var desc := TextEdit.new()
	desc.name = "LevelDescriptionEdit"
	desc.editable = false
	desc.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
	desc.custom_minimum_size = Vector2(0, 160)
	desc.size_flags_vertical = Control.SIZE_EXPAND_FILL
	desc.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(desc)
	_description = desc
	content.add_child(scroll)

	# --- 四个模式按钮：这里**只显示**，点不动（原版这四个在 GameSetup 里才是可循环的）---
	var options := GwUI.hbox("OptionsContainer", 10, content)
	for spec in [
		{"key": "axis", "name": "AxisModeButton"},
		{"key": "function", "name": "FunctionModeButton"},
		{"key": "turn", "name": "TurnModeButton"},
		{"key": "time", "name": "TimeModeButton"},
	]:
		var holder := AspectRatioContainer.new()
		holder.name = "AspectRatioContainer"
		holder.ratio = 1.0
		holder.stretch_mode = AspectRatioContainer.STRETCH_FIT
		holder.custom_minimum_size = Vector2(64, 64)
		var b := GwUI.mode_button("axis_option_3")
		b.name = String(spec.name)
		b.disabled = true
		holder.add_child(b)
		options.add_child(holder)
		_mode_buttons[String(spec.key)] = b

	var actions := GwUI.hbox("HBoxContainer3", 10, content)
	var spacer := GwUI.spacer(Vector2(0, 0))
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(spacer)
	var cancel := GwButton.make(" Cancel ", GwCommands.CANCEL_LEVEL)
	cancel.name = "GraphwarButton"
	cancel.custom_minimum_size = Vector2(120, 44)
	actions.add_child(cancel)
	var start := GwButton.make("  Start  ", GwCommands.START_LEVEL)
	start.name = "GraphwarButton2"
	start.custom_minimum_size = Vector2(120, 44)
	actions.add_child(start)

	(d.vbox as VBoxContainer).add_child(GwUI.ad_bar())

func on_enter() -> void:
	var app_node := app()
	var index := 0
	if app_node != null:
		index = int(app_node.campaign_selected_level)
	var level := GwCampaign.level(index)
	_title.text = String(level.title)
	_description.text = String(level.description)
	_update_mode_icons(level)

func _update_mode_icons(level: Dictionary) -> void:
	(_mode_buttons["axis"] as GwButton).icon = GwIcons.get_icon(GwModes.axis_icon(int(level.axis)))
	(_mode_buttons["axis"] as GwButton).tooltip_text = "Axis mode %d（4 档，语义未知）" % int(level.axis)
	(_mode_buttons["function"] as GwButton).icon = GwIcons.get_icon(GwModes.function_icon(int(level.function)))
	(_mode_buttons["function"] as GwButton).tooltip_text = "Function mode：%s" % ("f(x)" if int(level.function) == 0 else "dx/dy")
	(_mode_buttons["turn"] as GwButton).icon = GwIcons.get_icon(GwModes.turn_icon(int(level.turn)))
	(_mode_buttons["turn"] as GwButton).tooltip_text = "Turn mode：%s" % ("sequential" if int(level.turn) == 0 else "simultaneous")
	(_mode_buttons["time"] as GwButton).icon = GwIcons.get_icon(GwModes.time_icon(int(level.time)))
	(_mode_buttons["time"] as GwButton).tooltip_text = "Turn time：%s" % GwModes.time_label(int(level.time))
