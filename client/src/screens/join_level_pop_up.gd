extends GwScreen
## JoinLevelPopUp —— 关卡预览 / 开局确认。
##
## **真数据**（不再是占位）：标题 `Level N`、说明原文、四个模式图标的初值全部来自
## `GwCampaign.level(i)`，也就是 `docs/118` §4.1（标题 21/21）、§4.2（描述逐字节）、
## §4.3（四个模式 = 每关的 `imm32`）。
##
## 几何抄自恢复出的原版场景与脚本（1920×1160 实测值在括号里）：
##   `HBoxContainer2` 里 `MarginContainer(3) : Panel(6) : MarginContainer(3)` ⇒ 面板按内容撑到
##   **1434×910 @1920**（x=243..1677）；
##   `LevelTitleLabel.gd` 字号 `x/30`；`LevelDescriptionScrollContainer.gd` 字号 `x/40`；
##   `OptionsContainer.gd` 自身 `min.y = 0.25 × 父高`（实测 228 = 0.25×910）；
##   四个模式按钮各在一个 `AspectRatioContainer`（ratio 1.0/STRETCH_FIT）里 ⇒ **228×228 正方**，
##   左右各一个等宽占位；`Cancel`/`Start` 在 `HBoxContainer3` 里居中（实测 194×73 / 218×73）。
##
## 数据来源：`GwCampaign.LEVELS`（21 项，标注了二进制地址/文档出处）。

var _exit_pair: MarginContainer
var _cancel: GwButton
var _start: GwButton
var _title: Label
var _description: TextEdit
var _options: HBoxContainer
var _content: VBoxContainer
var _mode_buttons := {}

func _ready() -> void:
	font_divisor = 30        # LevelTitleLabel.gd 用 30；说明栏另行覆盖成 40
	super()

func build() -> void:
	var margin := MarginContainer.new()
	margin.name = "MarginContainer"
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 0)
	add_child(margin)
	var vbox := GwUI.vbox("VBoxContainer", 4, margin)

	# 左上角返回（原版 MarginContainer2/HBoxContainer/ExitButtonContainer）
	var top := GwUI.hbox("HBoxContainer", 4, vbox)
	_exit_pair = GwUI.exit_pair(GwCommands.CANCEL_LEVEL)
	top.add_child(_exit_pair)
	var top_spacer := GwUI.spacer(Vector2(0, 0))
	top_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top.add_child(top_spacer)

	# 面板宽 = 视口宽 × 6/(6+1+1) = 75% —— **按实测拟合**：原版 1920 下是 1434 px = 74.7%
	# （`orig_campaign_play.png` 的 GEOM）。原版 `.tscn` 写的是 3:6:3，但它的面板最小宽
	# 由 TextEdit 的最小宽撑到 ~790，最终仍落在 74.7%；这里直接用能复现该比例的 1:6:1。
	var row := GwUI.hbox("HBoxContainer2", 4, vbox)
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var side_left := GwUI.margin("MarginContainer", 0, row)
	side_left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	side_left.size_flags_stretch_ratio = 1.0
	var panel_holder := GwUI.margin("MarginContainer2", 0, row)
	panel_holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel_holder.size_flags_stretch_ratio = 6.0
	var side_right := GwUI.margin("MarginContainer3", 0, row)
	side_right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	side_right.size_flags_stretch_ratio = 1.0

	var panel := PanelContainer.new()
	panel.name = "Panel"
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	panel_holder.add_child(panel)
	_content = GwUI.vbox("VBoxContainer", 4, panel)

	_title = GwUI.label("Level 0", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_title.name = "LevelTitleLabel"
	_title.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	_content.add_child(_title)

	# 说明：只读 TextEdit（原版如此：`LevelDescriptionEdit`，theme = transparent_text_edit.tres）
	var desc_row := GwUI.hbox("HBoxContainer", 4, _content)
	desc_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	desc_row.size_flags_stretch_ratio = 2.0
	var desc_margin := GwUI.margin("MarginContainer", 0, desc_row)
	desc_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc_margin.size_flags_stretch_ratio = 20.0
	var scroll := GwUI.scroll("ScrollContainer")
	desc_margin.add_child(scroll)
	var desc := TextEdit.new()
	desc.name = "LevelDescriptionEdit"
	desc.editable = false
	desc.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
	desc.context_menu_enabled = false
	desc.shortcut_keys_enabled = false
	desc.selecting_enabled = false
	desc.virtual_keyboard_enabled = false
	desc.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.add_child(desc)
	_description = desc

	# 四个模式控件。**可点**（原版没有 disabled=true）——按下只是回写同一份数据，
	# 因为 JoinLevelPopUp 只是"确认页"，真正能循环的是 GameSetup。
	_options = GwUI.hbox("OptionsContainer", 4, _content)
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
		holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var b := GwButton.make("")
		b.name = String(spec.name)
		b.expand_icon = true
		b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.size_flags_vertical = Control.SIZE_EXPAND_FILL
		holder.add_child(b)
		_options.add_child(holder)
		_mode_buttons[String(spec.key)] = b
	# 首尾各一个等宽空占位（原版 OptionsContainer 里就是 6 个槽）
	var lead := GwUI.margin("MarginContainer", 0, _options)
	lead.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_options.move_child(lead, 0)
	var trail := GwUI.margin("MarginContainer2", 0, _options)
	trail.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	# Cancel / Start：底部居中、尺寸由文字决定（原版 " Cancel " / "  Start  "，含空格）
	var actions := GwUI.hbox("HBoxContainer3", 8, _content)
	var left_spacer := GwUI.spacer(Vector2(0, 0))
	left_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(left_spacer)
	var cancel_holder := GwUI.margin("MarginContainer", 0, actions)
	cancel_holder.size_flags_horizontal = Control.SIZE_SHRINK_CENTER   # 原版：按钮宽度由文字决定
	_cancel = GwButton.make(" Cancel ", GwCommands.CANCEL_LEVEL)
	_cancel.name = "GraphwarButton"
	cancel_holder.add_child(_cancel)
	var start_holder := GwUI.margin("MarginContainer2", 0, actions)
	start_holder.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_start = GwButton.make("  Start  ", GwCommands.START_LEVEL)
	_start.name = "GraphwarButton2"
	start_holder.add_child(_start)
	var right_spacer := GwUI.spacer(Vector2(0, 0))
	right_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(right_spacer)

	(vbox as VBoxContainer).add_child(GwUI.ad_bar())
	_content.resized.connect(_update_options_height)
	_options.resized.connect(_update_options_height)

## `OptionsContainer.gd`：`custom_minimum_size.y = 0.25 * 父高`（父 = 面板里的 VBox）。
func _update_options_height() -> void:
	if _options == null or _content == null:
		return
	var parent_h := _content.size.y
	# 面板还没布局（弹窗隐藏时 size 为 0）就先用"实测比例"兜底：
	# 1920×1160 下 HBoxContainer2 = 910 = 视口高 × 0.785（GEOM 实测）。
	if parent_h <= 1.0:
		parent_h = get_viewport_rect().size.y * 0.785
	var want := 0.25 * parent_h
	if absf(_options.custom_minimum_size.y - want) > 0.5:
		_options.custom_minimum_size = Vector2(0, want)
		# 改了最小高度会触发新一轮布局，再收敛一次（有 0.5px 门限，不会无限递归）
		_update_options_height.call_deferred()

func on_enter() -> void:
	var app_node := app()
	var index := 0
	if app_node != null:
		index = int(app_node.campaign_selected_level)
	var level := GwCampaign.level(index)
	_title.text = String(level.title)
	_description.text = String(level.description)
	_update_mode_icons(level)
	_update_options_height()
	_apply_desc_font()

func apply_font_scale() -> void:
	super.apply_font_scale()
	_apply_desc_font()
	if _built and _cancel != null:
		# `IpPopUpMargin.gd`：容器 margin = 10、子节点字号 = `x / 40`。
		# 核对：原版 @1920 Cancel = 194×73、Start = 218×73；我们按 x/40 得到 ~205×67 ✓
		# （此前量到 317 px 是因为两个 holder 设了 EXPAND，宽度被拉满 —— 已改成 SHRINK_CENTER）
		var fsize := maxi(8, int(get_viewport_rect().size.x / 40.0))
		for b in [_cancel, _start]:
			(b as GwButton).add_theme_font_size_override("font_size", fsize)
			var holder := (b as GwButton).get_parent()
			if holder is MarginContainer:
				for side in ["left", "top", "right", "bottom"]:
					(holder as MarginContainer).add_theme_constant_override("margin_" + side, 10)
	if _exit_pair != null:
		var k := get_viewport_rect().size.x * 0.05
		(_exit_pair as Control).custom_minimum_size = Vector2(k, k)

## `LevelDescriptionScrollContainer.gd`：说明栏字号 = `x / 40`（比标题小一档）。
func _apply_desc_font() -> void:
	if _description == null:
		return
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		return
	_description.add_theme_font_size_override("font_size", maxi(8, int(w / 40.0)))

func _update_mode_icons(level: Dictionary) -> void:
	(_mode_buttons["axis"] as GwButton).icon = GwIcons.get_icon(GwModes.axis_icon(int(level.axis)))
	(_mode_buttons["axis"] as GwButton).tooltip_text = "Axis mode %d（%s）" % [
		int(level.axis), GwModes.axis_label(int(level.axis))]
	(_mode_buttons["function"] as GwButton).icon = GwIcons.get_icon(GwModes.function_icon(int(level.function)))
	(_mode_buttons["function"] as GwButton).tooltip_text = "Function mode：%s" % (
		"f(x)" if int(level.function) == 0 else "dx/dy")
	(_mode_buttons["turn"] as GwButton).icon = GwIcons.get_icon(GwModes.turn_icon(int(level.turn)))
	(_mode_buttons["turn"] as GwButton).tooltip_text = "Turn mode：%s" % (
		"sequential" if int(level.turn) == 0 else "simultaneous")
	(_mode_buttons["time"] as GwButton).icon = GwIcons.get_icon(GwModes.time_icon(int(level.time)))
	(_mode_buttons["time"] as GwButton).tooltip_text = "Turn time：%s" % GwModes.time_label(int(level.time))
