extends GwScreen
## JoinLevelPopUp —— 关卡预览 / 开局确认。
##
## **真数据**（不再是占位）：标题 `Level N`、说明原文、四个模式图标的初值全部来自
## `GwCampaign.level(i)`，也就是 `docs/118` §4.1（标题 21/21）、§4.2（描述逐字节）、
## §4.3（四个模式 = 每关的 `imm32`）。
##
## 几何**逐节点照抄** `join_level_pop_up.tscn`（原版 41 节点），真值两条独立管线一致
## （`docs/145` §2）：
##   * **管线 C**：正版 `Graphwar.pck` + stock Godot 4.7.2 装 `main.tscn`，只显示本弹窗
##     ⇒ 引擎自报每个 `Control` 的 rect；
##   * **管线 A**：正版 `Graphwar.x86_64` 在 Xvfb 1920×1160 里点进去截图
##     ⇒ 面板/格子/按钮的黑边环**逐像素相同**。
## 1920×1160 的真值：
##
##   VBox(sep 4) ├ MarginContainer2  顶部返回键带   (0,0)   1920×**96**
##               ├ HBoxContainer2    面板带         (0,100) 1920×**956**   ← 1:10:1 的中间那条
##               │   ├ MarginContainer  左留白      239 宽（1 份）
##               │   ├ MarginContainer2 面板         (243,0) **1434×956**（6 份）
##               │   │    ├ Panel（4 边 3px 黑边 + 圆角 10）
##               │   │    └ VBoxContainer（**兄弟**，不是 PanelContainer —— 见下）
##               │   │         ├ LevelTitleLabel      1434×**90**
##               │   │         ├ HBoxContainer 说明   1434×**522**（= 剩余空间）
##               │   │         ├ OptionsContainer     1434×**239**（= 0.25 × 956）
##               │   │         └ HBoxContainer3 按钮  1434×**93**
##               │   └ MarginContainer3 右留白      239 宽
##               ├ HBoxContainer3    底部留白带      (0,1060) 1920×**96**
##               └ AdContainer       桌面 0 高        (0,1160) 1920×0
##
## 三个**曾经写错、现在按真值改回来**的点（`docs/145` §2.3）：
##
## 1. **底部那条 96 px 的留白带原来整个缺失** ⇒ 面板带吃掉它那一份，面板比原版高 96 px，
##    底边压到 y=1152（真值 1056）。用户那句"弹窗下面的部分太长"就是这条。
## 2. **面板不能是 `PanelContainer`**：它的 stylebox content margin 会把里面的 VBox 再内缩
##    3 px，于是标题/说明/四个格子整体下移 3 px。原版是 `Panel` 与 `VBoxContainer`
##    **两个兄弟**同时铺满 `MarginContainer2`（`docs/130` §3 同一条，七个弹窗通用）。
## 3. **`AxisModeButton` 的底色是白色**（`axis_button_theme.tres` 的四个态全指向
##    `axis_menu_icon_theme.tres` = 白底 + 3px 黑边 + 圆角 10）；另外三个模式按钮走
##    `game_menu_icon_theme.tres`（= `game_menu_icon_button_theme.tres`，
##    `#1A3B2D` + 3px 黑边 + 圆角 10）。我们以前四个都不带样式盒 ⇒ 全部落在默认深绿上。
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

	# ① 顶部返回键带：原版 `MarginContainer2 → HBoxContainer → ExitButtonContainer`。
	#    带高 = 返回键高（1920 宽 ⇒ 96）。它**不**吃剩余空间：真值那 96 就是它的最小高度。
	var top_band := GwUI.margin("MarginContainer2", 0, vbox)
	top_band.size_flags_vertical = Control.SIZE_FILL
	var top := GwUI.hbox("HBoxContainer", 4, top_band)
	top.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_exit_pair = GwUI.exit_pair(GwCommands.CANCEL_LEVEL)
	top.add_child(_exit_pair)

	# ② 面板带：三条竖带 1 : 6 : 1（真值 239 / 1434 / 239），它独占竖直剩余空间。
	var row := GwUI.hbox("HBoxContainer2", 4, vbox)
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.size_flags_stretch_ratio = PANEL_BAND_RATIO
	var side_left := GwUI.margin("MarginContainer", 0, row)
	side_left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	side_left.size_flags_stretch_ratio = 1.0
	var panel_holder := GwUI.margin("MarginContainer2", 0, row)
	panel_holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel_holder.size_flags_stretch_ratio = 6.0
	var side_right := GwUI.margin("MarginContainer3", 0, row)
	side_right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	side_right.size_flags_stretch_ratio = 1.0

	# 面板 = `Panel` + `VBoxContainer` **两个兄弟**（不是 PanelContainer）。
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
	_content.add_child(_title)

	# 说明行：原版 `HBoxContainer`（v=3、stretch_ratio 2）= 左留白(1) : IpPopUpMargin(20) : 右留白(1)。
	var desc_row := GwUI.hbox("HBoxContainer", 4, _content)
	desc_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	desc_row.size_flags_stretch_ratio = 2.0
	var desc_left := GwUI.margin("MarginContainer2", 0, desc_row)
	desc_left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc_left.size_flags_stretch_ratio = 1.0
	# `IpPopUpMargin.gd`：四边 margin 10 + 子节点字号 = 宽/40。
	var desc_margin := GwUI.margin("MarginContainer", 10, desc_row)
	desc_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc_margin.size_flags_stretch_ratio = 20.0
	desc_margin.set_meta("font_divisor", 40)
	var desc_right := GwUI.margin("MarginContainer3", 0, desc_row)
	desc_right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	desc_right.size_flags_stretch_ratio = 1.0

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
	# 底色按原版两份 `.tres`：`AxisModeButton` 走 `axis_button_theme.tres`（**白底**，
	# 它的 normal/hover/pressed/disabled 四个态全 = `axis_menu_icon_theme.tres`），
	# 另外三个走 `game_menu_icon_theme.tres`（`game_menu_icon_button_theme.tres`，`#1A3B2D`）。
	# 四个的 focus 都是 `StyleBoxEmpty`。真帧：第一格白底灰网格、其余三格深绿底
	# （`docs/145` §2.2 的 `docs/129` §4.5 同一份证据）。
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
		var on_white := String(spec.key) == "axis"
		var sb := GwThemeFactory.mode_icon_stylebox(on_white)
		for state in ["normal", "hover", "pressed", "disabled"]:
			b.add_theme_stylebox_override(state, sb)
		b.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
		holder.add_child(b)
		_options.add_child(holder)
		_mode_buttons[String(spec.key)] = b
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

	# ③ 底部留白带：原版 `HBoxContainer3` 是**空**容器，靠 1:10:1 拿到和返回键一样高的一份
	#    （1920×1160 真值 96）。这里把它钉成返回键的高度，理由见 `_band_height()`。
	_bottom_band = GwUI.hbox("HBoxContainer3", 4, vbox)
	_bottom_band.size_flags_vertical = Control.SIZE_FILL

	# ④ AdContainer：桌面端 0 高，但它前面那条 4 px 分隔**在**（真值 VBox 有 3 条分隔）
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
	# 面板还没布局（弹窗隐藏时 size 为 0）就先用"实测比例"兜底：
	# 1920×1160 下 HBoxContainer2 = 956 = 视口高 × 0.824…
	if parent_h <= 1.0:
		parent_h = _viewport_band_height()
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
	_apply_desc_font()

func apply_font_scale() -> void:
	super.apply_font_scale()
	_apply_desc_font()
	if _built and _cancel != null:
		# `IpPopUpMargin.gd`：容器 margin = 10、子节点字号 = `x / 40`。
		# 核对：原版 @1920 Cancel = 194×92、Start = 218×92（含 10 px 上下 margin 的那一行是 93 高）
		var fsize := maxi(8, int(get_viewport_rect().size.x / 40.0))
		for b in [_cancel, _start]:
			(b as GwButton).add_theme_font_size_override("font_size", fsize)
			var holder := (b as GwButton).get_parent()
			if holder is MarginContainer:
				for side in ["left", "top", "right", "bottom"]:
					(holder as MarginContainer).add_theme_constant_override("margin_" + side, 10)
	if _exit_pair != null:
		var k := _band_height()
		(_exit_pair as Control).custom_minimum_size = Vector2(0, k)
	if _bottom_band != null:
		_bottom_band.custom_minimum_size = Vector2(0, _band_height())

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
