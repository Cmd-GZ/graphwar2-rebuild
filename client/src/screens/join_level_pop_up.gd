extends GwScreen
## JoinLevelPopUp —— 关卡预览 / 开局确认。
##
## **真数据**（不再是占位）：标题 `Level N`、说明原文、四个模式图标的初值全部来自
## `GwCampaign.level(i)`，也就是 `docs/118` §4.1（标题 21/21）、§4.2（描述逐字节）、
## §4.3（四个模式 = 每关的 `imm32`）。
##
## ## 几何：本轮（docs/144）用**真值管线**重量了一遍，改了四处
##
## 真值来自 `docs/130` §1 的管线（正版 `Graphwar.pck` + `libgraphwarrust.so` 喂 stock
## Godot 4.7.2），`res://join_level_pop_up.tscn` 在 1920×1160 下逐节点实测：
##
## ```text
## MarginContainer(满屏) → VBoxContainer(sep 4)
##   ├ MarginContainer2  1920×96                 ← 返回键那一行（高 = 屏宽×0.05）
##   ├ HBoxContainer2    1920×956  EXPAND ratio 10
##   │    ├ MarginContainer    239  EXPAND ratio 1
##   │    ├ MarginContainer2  1434  EXPAND ratio 6   ← 3:6:3 ⇒ 1:6:1 之后落在 239/1434/239
##   │    │    └ Panel + VBoxContainer 1434×956
##   │    │         ├ LevelTitleLabel            1434×90   V-SHRINK-CENTER
##   │    │         ├ HBoxContainer              1434×535  EXPAND ratio 2
##   │    │         │    ├ MarginContainer2  64  (ratio 1)
##   │    │         │    ├ MarginContainer 1297  (ratio 20) → ScrollContainer 1277×515（内边距 10）
##   │    │         │    └ MarginContainer3  65  (ratio 1)
##   │    │         ├ OptionsContainer           1434×226  EXPAND ratio 0.5
##   │    │         │    6 格：垫片 + 4×AspectRatio + 垫片（各 235/236 宽）
##   │    │         │    └ 每个 AspectRatio(ratio 1, FIT) 里 226×226 的正方按钮
##   │    │         └ HBoxContainer3             1434×93
##   │    │              垫片 485 + Cancel 214 + Start 238 + 垫片 485（按钮 194×73 / 218×73）
##   ├ HBoxContainer3    1920×96                 ← **屏幕级还有一行 96**（无子节点、min=0）
##   └ AdContainer       1920×0
## ```
##
## 本轮按这条真值改的四件事：
##   1. **补上屏幕级那行 96 px**（`HBoxContainer3`）。它没有子节点、`combined_min=(0,0)`，
##      却拿到 96 —— 高度来自原版那次布局的剩余量分配，**机制未知**（推断：与返回键同高）。
##      少这一行，面板就变成 1056 高、整块往下顶到 y=1155，这就是用户说的"下面的部分太长"。
##   2. **说明栏两侧补 1:20:1 的垫片**（真值 64 / 1297 / 65，中间那层还有 10 px 内边距）。
##      以前只有一个 ratio 20 的容器，说明文字从面板左边框一路顶到右边框。
##   3. **`OptionsContainer` 是 EXPAND + ratio 0.5**，`custom_minimum_size.y = 0.25 × 父高`
##      （`OptionsContainer.gd` 的真源码，docs/101 §4.5）。真值场景冻结在 225.25
##      （= 0.25×901，是 ad 条折叠前那一次 `resized` 的残留），我们按公式求值 ⇒ 239。
##      这 14 px 是**已知残差**，写在 docs/144 §4 里。
##   4. **表示坐标模式的那一格是白底**：真值里 `AxisModeButton` 挂 `axis_button_theme.tres`
##      → normal 态 `bg=(1,1,1,1)` + 3 px 黑边 + 圆角 10；另外三格挂 `game_menu_icon_theme.tres`
##      → `bg=(0.102,0.2314,0.1765)`（= #1A3B2D）+ 同样的黑边。两份主题除底色外逐字段相同，
##      形状就是 `GwThemeFactory.mode_icon_stylebox(白底?)`（docs/129 §4.5 在大厅房间行里
##      已经用过同一对样式盒）。以前四格都没挂样式盒 ⇒ 白底那格丢成面板色。
##
## 数据来源：`GwCampaign.LEVELS`（21 项，标注了二进制地址/文档出处）。

var _exit_pair: MarginContainer
var _cancel: GwButton
var _start: GwButton
var _title: Label
var _description: TextEdit
var _options: HBoxContainer
var _content: VBoxContainer
var _bottom_band: HBoxContainer
var _mode_buttons := {}

## `HBoxContainer2` 在 `.tscn` 里的 `size_flags_stretch_ratio`。顶部/底部两条留白带
## 是 1（默认），所以竖直方向是 **1 : 10 : 1**（真值 96 / 956 / 96）。
const PANEL_BAND_RATIO := 10.0

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
	var top_holder := GwUI.margin("MarginContainer2", 0)
	top_holder.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var top := GwUI.hbox("HBoxContainer", 4)
	_exit_pair = GwUI.exit_pair(GwCommands.CANCEL_LEVEL)
	top.add_child(_exit_pair)
	top_holder.add_child(top)
	# 原版 `join_level_pop_up.tscn` 的 `MarginContainer/VBoxContainer/MarginContainer2`
	# 挂 `MarginContainerPopUpBack.gd`（安全区；桌面下 margin 全 0）
	GwViews.attach(top_holder, "MarginContainerPopUpBack.gd")
	vbox.add_child(top_holder)

	# 面板行。真值 239 : 1434 : 239 —— `.tscn` 写的是 3:6:3，但面板最小宽由 TextEdit
	# 撑住，最终落在 1:6:1；这里直接写 1:6:1 能复现同一个矩形（docs/130 实测 x=243..1677）。
	var row := GwUI.hbox("HBoxContainer2", 4, vbox)
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.size_flags_stretch_ratio = 10.0
	var side_left := GwUI.margin("MarginContainer", 0, row)
	side_left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	side_left.size_flags_stretch_ratio = 1.0
	var panel_holder := GwUI.margin("MarginContainer2", 0, row)
	panel_holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel_holder.size_flags_stretch_ratio = 6.0
	var side_right := GwUI.margin("MarginContainer3", 0, row)
	side_right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	side_right.size_flags_stretch_ratio = 1.0

	# 面板：原版是 `Panel` + `VBoxContainer` **两个兄弟**（都铺满 MarginContainer2），
	# 不是 PanelContainer —— PanelContainer 会用 stylebox 的 content_margin 再内缩 3 px，
	# 真值里 `Panel` 与 `VBoxContainer` 是同一个 1434×956 矩形（docs/130 的节点 dump）。
	var panel := Panel.new()
	panel.name = "Panel"
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	panel_holder.add_child(panel)
	_content = GwUI.vbox("VBoxContainer", 4, panel_holder)
	_content.set_anchors_preset(Control.PRESET_FULL_RECT)

	_title = GwUI.label("Level 0", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_title.name = "LevelTitleLabel"
	_title.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	_title.size_flags_vertical = Control.SIZE_SHRINK_CENTER   # 真值 flags=V-SHRINK-CENTER
	GwViews.attach(_title, "LevelTitleLabel.gd")   # 字号 = 宽/30
	_content.add_child(_title)

	# 说明行：真值三个子节点 + 中间那层 10 px 内边距 ⇒ 64 / 1297 / 65。
	var desc_row := GwUI.hbox("HBoxContainer", 4, _content)
	desc_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	desc_row.size_flags_stretch_ratio = 2.0
	var desc_pad_l := GwUI.margin("MarginContainer2", 0, desc_row)
	desc_pad_l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc_pad_l.size_flags_stretch_ratio = 1.0
	var desc_margin := GwUI.margins("MarginContainer", 10, 10, 10, 10)
	desc_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc_margin.size_flags_stretch_ratio = 20.0
	GwViews.attach(desc_margin, "IpPopUpMargin.gd")   # margin 10 + 子节点字号 = 宽/40
	desc_row.add_child(desc_margin)
	var desc_right := GwUI.margin("MarginContainer3", 0, desc_row)
	desc_right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc_right.size_flags_stretch_ratio = 1.0

	var scroll := GwUI.scroll("ScrollContainer")
	GwViews.attach(scroll, "LevelDescriptionScrollContainer.gd")   # 子节点字号 = 宽/40
	desc_margin.add_child(scroll)
	# 说明栏的 TextEdit 是随后加的 ⇒ 复用原版自己的刷新函数把字号补上
	scroll.call_deferred("update_child_sizes")
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
	# 说明栏**不能有底色**：原版 `LevelDescriptionEdit` 的 theme 是
	# `transparent_text_edit.tres`，整块面板是统一的 `#1A3B2D`（观测：原版真帧
	# `.agent-work/playfit/shots/orig_join_level.png` 在面板矩形内取样，
	# y=130/200/300/400 全是 `(26,59,45)`，没有任何横贯色带）。
	#
	# 之前只覆盖了主题里的 `TextEdit/normal`，但控件设了 `editable = false`
	# ⇒ Godot 实际画的是 **`read_only`** 那套样式（主题没覆盖 ⇒ 落回默认主题的
	# 深色底 + `font_readonly_color` 暗绿字），于是面板上多出一条深色带、
	# 说明文字也发绿（实测底色 `(26,49,39)`、字色 `(124,136,131)`）。
	# 原版真帧的说明文字是 `(223,223,223)`，所以这里把两样都显式钉死。
	for slot in ["normal", "focus", "read_only"]:
		desc.add_theme_stylebox_override(slot, StyleBoxEmpty.new())
	desc.add_theme_color_override("font_readonly_color", Color8(223, 223, 223))
	desc.add_theme_color_override("font_color", Color8(223, 223, 223))
	scroll.add_child(desc)
	_description = desc

	# 四个模式控件。**可点**（原版没有 disabled=true）——按下只是回写同一份数据，
	# 因为 JoinLevelPopUp 只是"确认页"，真正能循环的是 GameSetup。
	#
	# 底色：真值里只有 **第一格（坐标模式）是白底**，另外三格是面板色。两格用的主题
	# （`axis_button_theme.tres` / `game_menu_icon_theme.tres`）除底色外逐字段相同，
	# 所以这里挂同一份 `mode_icon_stylebox()` 的两个变体即可（docs/144 §4）。
	_options = GwUI.hbox("OptionsContainer", 4, _content)
	_options.size_flags_vertical = Control.SIZE_EXPAND_FILL   # 真值 flags=V-EXPAND
	_options.size_flags_stretch_ratio = 0.5                   # 真值 ratio=0.50
	var mode_index := 0
	for spec in [
		{"key": "axis", "name": "AxisModeButton"},
		{"key": "function", "name": "FunctionModeButton"},
		{"key": "turn", "name": "TurnModeButton"},
		{"key": "time", "name": "TimeModeButton"},
	]:
		var holder := AspectRatioContainer.new()
		holder.name = "AspectRatioContainer" if mode_index == 0 else "AspectRatioContainer%d" % (mode_index + 1)
		holder.ratio = 1.0
		holder.stretch_mode = AspectRatioContainer.STRETCH_FIT
		holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var b := GwButton.make("")
		b.name = String(spec.name)
		b.expand_icon = true
		b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.size_flags_vertical = Control.SIZE_EXPAND_FILL
		var sb := GwThemeFactory.mode_icon_stylebox(mode_index == 0)
		for state in ["normal", "hover", "pressed", "disabled"]:
			b.add_theme_stylebox_override(state, sb)
		holder.add_child(b)
		_options.add_child(holder)
		_mode_buttons[String(spec.key)] = b
		mode_index += 1
	# 首尾各一个等宽空占位（原版 OptionsContainer 里就是 6 个槽）
	var lead := GwUI.margin("MarginContainer", 0, _options)
	lead.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_options.move_child(lead, 0)
	var trail := GwUI.margin("MarginContainer2", 0, _options)
	trail.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	# Cancel / Start：底部居中、尺寸由文字决定（原版 " Cancel " / "  Start  "，含空格）。
	# `HBoxContainer3` **没有** separation 覆盖 ⇒ 默认 4；两个按钮各在一个 10 px 的
	# `IpPopUpMargin` 里 ⇒ 视觉间距 4+10+10 = **24 px**（真帧两键之间的空档正好 24 列：
	# Cancel 右黑边 933..935、Start 左黑边 960..962）。
	var actions := GwUI.hbox("HBoxContainer3", GwUI.SEPARATION, _content)
	var left_spacer := GwUI.spacer(Vector2(0, 0))
	left_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(left_spacer)
	var cancel_holder := GwUI.margin("MarginContainer", 10)
	cancel_holder.size_flags_horizontal = Control.SIZE_SHRINK_CENTER   # 原版：按钮宽度由文字决定
	_cancel = GwButton.make(" Cancel ", GwCommands.CANCEL_LEVEL)
	_cancel.name = "GraphwarButton"
	cancel_holder.add_child(_cancel)
	GwViews.attach(cancel_holder, "IpPopUpMargin.gd")
	actions.add_child(cancel_holder)
	cancel_holder.call_deferred("update_child_sizes")
	var start_holder := GwUI.margin("MarginContainer2", 10)
	start_holder.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_start = GwButton.make("  Start  ", GwCommands.START_LEVEL)
	_start.name = "GraphwarButton2"
	start_holder.add_child(_start)
	GwViews.attach(start_holder, "IpPopUpMargin.gd")
	actions.add_child(start_holder)
	start_holder.call_deferred("update_child_sizes")
	var right_spacer := GwUI.spacer(Vector2(0, 0))
	right_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(right_spacer)

	# **屏幕级**那一行 96 px（真值 `HBoxContainer3`：无子节点、combined_min=(0,0)，却拿到 96）。
	# 它把面板从 1056 压回 956 —— 没有它，整块面板会一直顶到屏幕底边
	# （用户实测的"弹窗下面的部分太长"就是这个）。高度在 `apply_font_scale()` 里按
	# 屏宽×0.05 设（1920 下 = 96，与返回键同高）。**机制未观测**，只观测到矩形。
	_bottom_band = GwUI.hbox("HBoxContainer3", 4, vbox)

	(vbox as VBoxContainer).add_child(GwUI.ad_bar())

	_content.resized.connect(_update_options_height)
	_options.resized.connect(_update_options_height)

## 上下两条留白带的高度（1920 宽 ⇒ 96）。
##
## 真值（`docs/145` §2.1，两条管线一致）：顶部带 96 = `ExitButtonContainer` 的高度
## （文本 `"     "` 五个空格 @ 字号 宽/30 ⇒ 146×96）；底部带同为 96。
## 我们的返回键对由 `GwUI.exit_pair()` 给 `custom_minimum_size = (0.05×屏宽)²`，
## 1920 下正是 96 —— 所以两条带都用同一个数，和真值逐像素对得上。
## **推断**（缺证据）：换成别的宽高比时，原版底部带其实是 1:10:1 分出来的份额
## （1/12 的可分配高度），不见得还等于返回键高度。桌面 16:9 下两者相等，没有分歧。
func _band_height() -> float:
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		w = float(DisplayServer.get_display_safe_area().size.x)
	if w <= 0.0:
		return 96.0
	return floorf(w * 0.05)

## `OptionsContainer.gd`：`custom_minimum_size.y = 0.25 * 父高`（父 = 面板里的 VBox）。
## 真值 239 = 0.25 × 956。**注意**：这行的 min 一旦定下来，剩余高度就全部落到说明行
## （真值 522），所以 options 行**不能**同时吃剩余空间 —— 否则它会跟著变大
## （`docs/145` §2.4 记了这条与 `.tscn` 里 `size_flags_vertical = 3` 的差别）。
func _update_options_height() -> void:
	if _options == null or _content == null:
		return
	var parent_h := _content.size.y
	# 面板还没布局（弹窗隐藏时 size 为 0）就先用**真值结构**兜底：
	# 面板高 = 视口高 − 2×（屏宽×0.05 的上/下两行）− 3×4 的分隔 ⇒ 1920×1160 下 = 956。
	if parent_h <= 1.0:
		var vp := get_viewport_rect().size
		parent_h = vp.y - 2.0 * (vp.x * 0.05) - 12.0
	var want := 0.25 * parent_h
	if absf(_options.custom_minimum_size.y - want) > 0.5:
		_options.custom_minimum_size = Vector2(0, want)
		# 改了最小高度会触发新一轮布局，再收敛一次（有 0.5px 门限，不会无限递归）
		_update_options_height.call_deferred()

## 面板带在"还没布局"时的预测高度：视口高 − 2×带高 − 3×4（真值 1160−96−96−12 = 956）。
func _viewport_band_height() -> float:
	return maxf(1.0, get_viewport_rect().size.y - 2.0 * _band_height() - 12.0)

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

func apply_font_scale() -> void:
	super.apply_font_scale()
	# Cancel / Start 两个壳的字号与 10 px margin 现在由原版 `IpPopUpMargin.gd` 自己写
	# （`client/src/views/`），这里不再重复设；1920 下仍是 194×92 / 218×92。
	if _exit_pair != null:
		var k := get_viewport_rect().size.x * 0.05
		(_exit_pair as Control).custom_minimum_size = Vector2(k, k)
		# 屏幕级那一行 96（= 屏宽×0.05，与返回键同高）。真值矩形 1920×96。
		if _bottom_band != null:
			_bottom_band.custom_minimum_size = Vector2(0, k)

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
