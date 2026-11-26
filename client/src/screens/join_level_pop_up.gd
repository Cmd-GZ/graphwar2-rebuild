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
## Cancel / Start：**原生 `GraphwarButton`**（原版 `.tscn` 里的节点）。
var _cancel: Button
var _start: Button
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

## 节点树来自原版 `.tscn`：`client/scenes/screens/join_level_pop_up.tscn` 与
## `recovered-src/join_level_pop_up.tscn` **逐字节相同**（258 行 / 42 节点）。我方 `build()` 只取句柄：
##
##   MarginContainer/VBoxContainer
##     ├ MarginContainer2 [v=3]                        ← 原版 `MarginContainerPopUpBack.gd`（安全区）
##     │    └ HBoxContainer → ExitButtonContainer{ClipButton, AnimationRect, LongPressButton}
##     ├ HBoxContainer2 [v=3, ratio 10]                ← 1 : 10 : 1 三条竖带的中段
##     │    └ MarginContainer2 → Panel + VBoxContainer
##     │         ├ LevelTitleLabel（原版 `LevelTitleLabel.gd`，字号 宽/30）
##     │         ├ HBoxContainer → M2 · MarginContainer(10)/
##     │         │     ScrollContainer（`LevelDescriptionScrollContainer.gd`）/LevelDescriptionEdit
##     │         │  ← TextEdit 挂 `transparent_text_edit.tres`
##     │         ├ OptionsContainer（原版 `OptionsContainer.gd`）
##     │         │     4 × AspectRatioContainer/<模式按钮>；axis 挂 `axis_button_theme.tres`（白卡），
##     │         │     其余三个挂 `game_menu_icon_theme.tres`（深绿卡 + 3 px 黑边 + 圆角 10）；
##     │         │     第四格的 AspectRatioContainer 挂 `TurnLimitText.gd`
##     │         └ HBoxContainer3 → M3 · IpPopUpMargin(Cancel) · M2/IpPopUpMargin(Start) · M4
##     ├ HBoxContainer3 [v=3]                           ← 屏幕级留白带（真值 1920×96）
##     └ AdContainer
##
## 模式按钮**没有** `input_text`（原版这一屏只是**展示**关卡规则，不给改）⇒ 我方只把
## 关卡数据画上去（图标 + tooltip），逻辑仍在本文件（docs/06 §92）。
const ROOT := "MarginContainer/VBoxContainer"
const CONTENT := ROOT + "/HBoxContainer2/MarginContainer2/VBoxContainer"

func build() -> void:
	_exit_pair = get_node_or_null(ROOT + "/MarginContainer2/HBoxContainer/ExitButtonContainer") 		as MarginContainer
	_content = get_node_or_null(CONTENT) as VBoxContainer
	if _content == null:
		push_error("[gw2] join_level_pop_up.tscn 里找不到 %s" % CONTENT)
		return
	_title = get_node_or_null(CONTENT + "/LevelTitleLabel") as Label
	_description = get_node_or_null(
		CONTENT + "/HBoxContainer/MarginContainer/ScrollContainer/LevelDescriptionEdit") as TextEdit
	_options = get_node_or_null(CONTENT + "/OptionsContainer") as HBoxContainer
	_bottom_band = get_node_or_null(ROOT + "/HBoxContainer3") as HBoxContainer
	# `OptionsContainer.gd` 只在**自己**的 `resized` 里重算 `min.y = 0.25 × 父高`；父容器高度变了
	# 而自己尺寸没变的那一刻它会留在旧值上（本轮实测差 2.4 px = 0.25×9.6）。这里把它父容器的
	# `resized` 也接上，仍然只调**它自己的** `set_minimum_size()`（不另写算式）。
	if not _content.resized.is_connected(_resync_options_height):
		_content.resized.connect(_resync_options_height)
	_cancel = get_node_or_null(CONTENT + "/HBoxContainer3/MarginContainer/GraphwarButton") as Button
	_start = get_node_or_null(CONTENT + "/HBoxContainer3/MarginContainer2/GraphwarButton") as Button
	if _title == null or _description == null or _options == null:
		push_error("[gw2] join_level_pop_up.tscn 结构不完整（title/description/options）")
		return
	# 四个模式按钮在 `OptionsContainer/AspectRatioContainer*/` 里 ⇒ 递归找名字。
	# ⚠️ 节点名与 GameSetup 屏**不同**：这里是 `TimeModeButton`（那边叫 `TurnTimeButton`）。
	for spec in [
		{"key": "axis", "name": "AxisModeButton"},
		{"key": "function", "name": "FunctionModeButton"},
		{"key": "turn", "name": "TurnModeButton"},
		{"key": "time", "name": "TimeModeButton"},
	]:
		var b := _options.find_child(String(spec["name"]), true, false) as Button
		if b == null:
			push_error("[gw2] join_level_pop_up.tscn 里找不到 %s" % String(spec["name"]))
			continue
		_mode_buttons[String(spec["key"])] = b


func _band_height() -> float:
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		w = float(DisplayServer.get_display_safe_area().size.x)
	if w <= 0.0:
		return 96.0
	return floorf(w * 0.05)

## 面板带在"还没布局"时的预测高度：视口高 − 2×带高 − 3×4（真值 1160−96−96−12 = 956）。
func _viewport_band_height() -> float:
	return maxf(1.0, get_viewport_rect().size.y - 2.0 * _band_height() - 12.0)

## 父容器换了高度 ⇒ 让**原版** `OptionsContainer.gd` 自己再算一次（它自己的函数名）。
## 这里**没有**任何高度算式：算的是 `recovered-src/OptionsContainer.gd::set_minimum_size`。
func _resync_options_height() -> void:
	if _options != null and _options.has_method("set_minimum_size"):
		_options.call_deferred("set_minimum_size")

func on_enter() -> void:
	var app_node := app()
	var index := 0
	if app_node != null:
		index = int(app_node.campaign_selected_level)
	var level := GwCampaign.level(index)
	_title.text = String(level.title)
	_description.text = String(level.description)
	_update_mode_icons(level)
	# 高度由**原版** `OptionsContainer.gd` 自己算；这里只催它一次（复用它的
	# `set_minimum_size()`，不是另写一份算式）。跨屏重进时父高会变，而 `resized`
	# 只在尺寸真的变化时才发 —— 门没变就什么都不会发，所以显式催一次。
	if _options != null:
		_options.call_deferred("set_minimum_size")

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
	(_mode_buttons["axis"] as Button).icon = GwIcons.get_icon(GwModes.axis_icon(int(level.axis)))
	(_mode_buttons["axis"] as Button).tooltip_text = "Axis mode %d（%s）" % [
		int(level.axis), GwModes.axis_label(int(level.axis))]
	(_mode_buttons["function"] as Button).icon = GwIcons.get_icon(GwModes.function_icon(int(level.function)))
	(_mode_buttons["function"] as Button).tooltip_text = "Function mode：%s" % (
		"f(x)" if int(level.function) == 0 else "dx/dy")
	(_mode_buttons["turn"] as Button).icon = GwIcons.get_icon(GwModes.turn_icon(int(level.turn)))
	(_mode_buttons["turn"] as Button).tooltip_text = "Turn mode：%s" % (
		"sequential" if int(level.turn) == 0 else "simultaneous")
	(_mode_buttons["time"] as Button).icon = GwIcons.get_icon(GwModes.time_icon(int(level.time)))
	(_mode_buttons["time"] as Button).tooltip_text = "Turn time：%s" % GwModes.time_label(int(level.time))

