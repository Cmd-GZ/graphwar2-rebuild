extends GwScreen
## ConfirmPlayExitPopUp —— 退出对局确认。
##
## 本轮（docs/130）照恢复出的 `confirm_exit_pop_up.tscn` **逐节点、逐属性**重写，
## 并用"原版 `Graphwar.pck` 直接实例化 + 同版本引擎渲染"的逐像素真值核对。
## 1920×1160 下量到的矩形（绝对像素，来自引擎自己的 `Control.position/size`）：
##
##   VBoxContainer 的三条竖带  1920×382 / 1920×383 / 1920×382（sep 4）
##   面板外框 (482,386) 956×383     ← HBoxContainer2 的**中段**，三列比例 1:2:1
##   文案 Label (492,485) 936×67    ← 水平居中 + 垂直居中
##   No 按钮  (687,665) 154×73
##   Yes 按钮 (1061,665) 171×73     ← 两个按钮**分开**、整行居中
##   返回键   (0,0) 146×96         ← **挂在弹窗根节点上**的浮层，不在面板里
##
## 用户实测的差异全部来自这里：
##   ① 面板偏窄偏左 → 我们原来按内容最小尺寸（560×220）居中，不是原版的 1:2:1 三列；
##   ② 文案靠上偏左 → 原版 Label 的 h/v alignment 都居中，且外面套 `IpPopUpMargin.gd`
##      的 10 px 内边距（字号 = 宽/40）；
##   ③ No/Yes 挤在右上 → 原版放在**面板中段**，左右各一个弹性空格夹着；
##   ④ 返回键在面板左下角 → 原版根本不在面板里：它是根节点上的 `MarginContainer2`
##      （`MarginContainerPopUpBack.gd`，桌面下 margin 全 0）+ `ExitButtonContainer`
##      ⇒ 位置恒为 (0,0)。
##
## 观测（`docs/92` §4.11）：对局里 `ClipButton` 单击**不弹**框，`LongPressButton` 单击**弹**框
## —— `ClipButton` 只是动画层。`GwUI.exit_pair()` 照抄了这个分工。

func _ready() -> void:
	# `IpPopUpMargin.gd`：内边距 10，字号 = 屏幕宽/40（文案与两个按钮都吃这个字号）。
	font_divisor = 40
	super()

func build() -> void:
	var frame := GwUI.frame(self)
	var bands: VBoxContainer = frame.vbox

	# 竖直三段带（原版 VBoxContainer 的三个 `[v=3]` 子项，各占 1/3）。底部那个 0 高的
	# AdContainer 仍在（它贡献的 4 px 分隔也在真值里 ⇒ 382/383/382 而不是 386）。
	_popup_band(bands, "HBoxContainer")
	var panel_band := _popup_band(bands, "HBoxContainer2")
	_popup_band(bands, "HBoxContainer3")
	bands.add_child(GwUI.ad_bar())

	# 面板带内部：左空白 1 : 面板 2 : 右空白 1（`size_flags_stretch_ratio = 2.0`）
	_expander(panel_band, "MarginContainer", 1.0)
	var panel_col := _expander(panel_band, "MarginContainer2", 2.0)
	_expander(panel_band, "MarginContainer3", 1.0)

	# 面板：原版是 `Panel` + `VBoxContainer` **两个兄弟**（都铺满 MarginContainer2），
	# 不是 PanelContainer —— PanelContainer 会用 stylebox 的 content_margin 再内缩一次。
	var panel := Panel.new()
	panel.name = "Panel"
	panel_col.add_child(panel)
	var content := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, panel_col)

	# 面板内部竖直比例：空白 1 : 文案行(自适应) : 空白 1 : 按钮行(自适应) : 空白 0.2
	_grow_spacer(content, "MarginContainer2", 1.0)

	var label_row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, content)
	var label_box := GwUI.margin("MarginContainer", 10, label_row)   # `IpPopUpMargin.gd`
	label_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var label := GwUI.label("Are you sure you want to exit?", 0, HORIZONTAL_ALIGNMENT_CENTER)
	label.name = "Label"
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label_box.add_child(label)

	_grow_spacer(content, "MarginContainer", 1.0)

	var buttons := GwUI.hbox("HBoxContainer3", GwUI.SEPARATION, content)
	_grow_spacer(buttons, "MarginContainer3", 1.0)
	_ip_margin(buttons, "MarginContainer").add_child(
		GwButton.make("  No  ", GwCommands.CANCEL_EXIT_PLAY))
	_grow_spacer(buttons, "MarginContainer5", 1.0)
	_ip_margin(buttons, "MarginContainer2").add_child(
		GwButton.make("  Yes  ", GwCommands.EXIT_PLAY))
	_grow_spacer(buttons, "MarginContainer4", 1.0)

	_grow_spacer(content, "MarginContainer3", 0.2)

	# 返回键：原版是**根节点**上的 `MarginContainer2`（挂 `MarginContainerPopUpBack.gd`）。
	# 那个脚本只在"窗口超出屏幕安全区"时才写 margin（桌面 = 0），所以它恒在 (0,0)。
	var overlay := GwUI.margin("MarginContainer2", 0, self)
	overlay.add_child(GwUI.exit_pair(GwCommands.CANCEL_EXIT_PLAY))

# ---------------------------------------------------------------- 原版骨架的小工具

## 原版每条竖带都是 `HBoxContainer` + `size_flags_vertical = 3`。
func _popup_band(parent: VBoxContainer, node_name: String) -> HBoxContainer:
	var b := GwUI.hbox(node_name, GwUI.SEPARATION, parent)
	b.size_flags_vertical = Control.SIZE_EXPAND_FILL
	return b

## 只按**宽度**分比例的空 `MarginContainer`（原版的左右留白，竖直是 FILL）。
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

## `IpPopUpMargin.gd` 包一层：margin 全 10，**不**参与弹性分配（字号由本屏除数给）。
func _ip_margin(parent: HBoxContainer, node_name: String) -> MarginContainer:
	var m := GwUI.margin(node_name, 10, parent)
	m.size_flags_horizontal = Control.SIZE_FILL
	return m
