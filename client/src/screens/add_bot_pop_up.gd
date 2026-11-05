extends GwScreen
## AddBotPopUp —— 加 Bot（六档难度）。
##
## 本轮（docs/130）照恢复出的 `add_bot_pop_up.tscn` 逐节点重写，并用"原版 pck 直接
## 实例化 + 同版本引擎"的逐像素真值核对。1920×1160 下量到的矩形（绝对像素）：
##
##   返回键带 (0,0)      1920×96      ← 返回键就在这条带里（**不是**根节点浮层）
##   面板     (243,100)  1434×956     ← 三列 1:6:1；竖直 1:10:1 三段带的中段
##   六个格子 (508/829/1150, 200 与 584) 261×261
##   Cancel   (853,963)  194×73       ← 面板底、水平居中
##
## 原版结构（`.tscn` 45 节点，与本文件的节点名一一对应）：
##
##   MarginContainer/VBoxContainer
##     ├ MarginContainer [v=3]            ← 带 `MarginContainerPopUpBack.gd`
##     │   └ HBoxContainer [v=3] → ExitButtonContainer{ClipButton, AnimationRect, LongPressButton}
##     ├ HBoxContainer2 [v=3, ratio 10]
##     │   ├ MarginContainer [h=3] · MarginContainer2 [h=3, ratio 6] · MarginContainer3 [h=3]
##     │   └ 后者里：Panel + VBoxContainer
##     │        ├ MarginContainer [h=3,v=3, ratio 0.1]          ← 顶部内留白
##     │        ├ OptionsContainer  [v=3]  ← M(1) AR AR2 M4(.2) AR4 M3(.2) AR4 M5(1)
##     │        ├ OptionsContainer2 [v=3]  ← 同上（原版两行的子节点**同名**，靠父节点区分）
##     │        └ HBoxContainer3 → M3[h=3,v=3] · Ip(M){Cancel} · M4[h=3,v=3]
##     ├ HBoxContainer3 [v=3]             ← 空底带
##     └ AdContainer（运行时 0 高）
##
## 两处以前做错、真帧上直接看得见的：
##   * **没有标题**：原版这块面板里没有任何 "ADD BOT" 文字，我们以前自己加了一行；
##   * 格子是 `AspectRatioContainer`（比例 1、默认 `stretch_mode`）里塞一个只有图标的
##     `GraphwarButton`（`expand_icon = true`），**没有** `custom_minimum_size`：
##     格子尺寸由 1:.2 的空隙比例 + 行高决定（261×261），不是我们钉的 96/120。
##
## 图标：原版 `skins/bot_0..5.svg`（512×512，画面只占中间约 40%）。
## **`bot_N` ↔ 六档难度的对应关系没有观测**（`docs/92` §4.6），所以这里只按顺序摆放，
## tooltip 里明确写"对应关系未证实"。

func _ready() -> void:
	font_divisor = 40        # `IpPopUpMargin.gd`（Cancel 那一层）
	super()

func build() -> void:
	var frame := GwUI.frame(self)
	var bands: VBoxContainer = frame.vbox

	# 第一条带：返回键（原版放在 VBox 的第一段里，而不是像其它 5 个弹窗那样挂根节点）
	var back_band := _band(bands, "MarginContainer", 1.0)
	var back_row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, back_band)
	back_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	back_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	back_row.add_child(GwUI.exit_pair(GwCommands.CANCEL_ADD_BOT))

	# 中间带：面板（ratio 10 : 上下两条带各 1）
	var panel_band := _band(bands, "HBoxContainer2", 10.0)
	_expander(panel_band, "MarginContainer", 1.0)
	var panel_col := _expander(panel_band, "MarginContainer2", 6.0)
	_expander(panel_band, "MarginContainer3", 1.0)

	var panel := Panel.new()
	panel.name = "Panel"
	panel_col.add_child(panel)
	var content := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, panel_col)

	# 面板内部：顶部留白 0.1 : 两行格子各 1 : Cancel 行（自适应）
	_grow_spacer(content, "MarginContainer", 0.1)
	_options_row(content, "OptionsContainer", 0)
	_options_row(content, "OptionsContainer2", 3)
	_build_cancel_row(content)

	# 第三条带：空底带（原版就是这样，什么都不放）
	var bottom_band := _band(bands, "HBoxContainer3", 1.0)
	bottom_band.mouse_filter = Control.MOUSE_FILTER_IGNORE
	bands.add_child(GwUI.ad_bar())

## 一行三个格子。原版两行的子节点名**完全一样**（`AspectRatioContainer` /
## `AspectRatioContainer2` / `AspectRatioContainer4`），空格子的比例是 1 : .2 : .2 : 1。
func _options_row(content: VBoxContainer, node_name: String, first_tier: int) -> void:
	var row := GwUI.hbox(node_name, GwUI.SEPARATION, content)
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_expander(row, "MarginContainer", 1.0)
	for i in 3:
		if i > 0:
			_expander(row, "MarginContainer3" if i == 2 else "MarginContainer4", 0.2)
		# 原版命名不整齐：没有 `AspectRatioContainer3`，第三列叫 `AspectRatioContainer4`
		var ar_name := "AspectRatioContainer" + ("" if i == 0 else ("2" if i == 1 else "4"))
		var ar := AspectRatioContainer.new()
		ar.name = ar_name
		ar.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.add_child(ar)
		var tier := first_tier + i
		var b := GwButton.make("")
		b.name = "AddBotButton"
		b.input_text = "%s%d" % [GwCommands.ADD_BOT_PREFIX, tier]
		b.icon = GwIcons.get_icon("bot_%d" % tier)
		b.expand_icon = true
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.tooltip_text = "%s\nbot_%d 与难度的对应关系**未观测**（docs/92 §4.6），此处只按顺序摆放" % [
			GwGameState.BOT_TIER_LABELS[tier], tier]
		ar.add_child(b)
	_expander(row, "MarginContainer5", 1.0)

## Cancel 行：左右两个 `[h=3,v=3]` 弹性空格夹住一个 `IpPopUpMargin` 壳。
func _build_cancel_row(content: VBoxContainer) -> void:
	var row := GwUI.hbox("HBoxContainer3", GwUI.SEPARATION, content)
	_grow_spacer(row, "MarginContainer3", 1.0)
	var shell := GwUI.margin("MarginContainer", 10, row)
	shell.size_flags_horizontal = Control.SIZE_FILL
	shell.add_child(GwButton.make(" Cancel ", GwCommands.CANCEL_ADD_BOT))
	_grow_spacer(row, "MarginContainer4", 1.0)

# ---------------------------------------------------------------- 原版骨架的小工具

## 竖带：`HBoxContainer` + `size_flags_vertical = 3`（原版三条带比例 1 : 10 : 1）。
func _band(parent: VBoxContainer, node_name: String, ratio: float) -> HBoxContainer:
	var b := GwUI.hbox(node_name, GwUI.SEPARATION, parent)
	b.size_flags_vertical = Control.SIZE_EXPAND_FILL
	b.size_flags_stretch_ratio = ratio
	return b

## 只按宽度分比例的空 `MarginContainer`（原版左右留白，竖直 FILL）。
func _expander(parent: Container, node_name: String, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	return m

## 两个方向都 EXPAND 的空 `MarginContainer`（`[h=3,v=3]`）。
func _grow_spacer(parent: Container, node_name: String, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	m.size_flags_vertical = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	return m
