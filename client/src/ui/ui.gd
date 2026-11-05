extends RefCounted
class_name GwUI
## 界面积木。
##
## 为什么用代码搭界面而不是手写 `.tscn`：
##   1. 原版的结构是"静态骨架 + 运行时生成动态部分"，动态部分（21 个关卡按钮、
##      玩家界面 244 个节点、57 条回放条目）**本来就是代码建的**（`docs/104` §5.3）；
##   2. 重建要求**不把恢复出的 `.tscn` 原样搬进来**，手写 15 个场景文件反而更难审查；
##   3. 所有布局数值集中在几个函数里，便于与 `docs/101` §5 的数值表逐条对照。
## 每屏仍各有一个 `scenes/screens/*.tscn`（只有根节点 + 脚本），以便编辑器里打开。

const FOOTER_AD_HEIGHT := 60.0   # 观测：11 个界面的 AdContainer 都是 custom_minimum_size=(0,60)

static func vbox(name_: String = "VBoxContainer", separation: int = 8, parent: Node = null) -> VBoxContainer:
	var n := VBoxContainer.new()
	n.name = name_
	n.add_theme_constant_override("separation", separation)
	n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	n.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if parent != null:
		parent.add_child(n)
	return n

static func hbox(name_: String = "HBoxContainer", separation: int = 8, parent: Node = null) -> HBoxContainer:
	var n := HBoxContainer.new()
	n.name = name_
	n.add_theme_constant_override("separation", separation)
	n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if parent != null:
		parent.add_child(n)
	return n

static func margin(name_: String = "MarginContainer", px: int = 8, parent: Node = null) -> MarginContainer:
	return margins(name_, px, px, px, px, parent)

static func margins(name_: String, left: int, top: int, right: int, bottom: int, parent: Node = null) -> MarginContainer:
	var n := MarginContainer.new()
	n.name = name_
	n.add_theme_constant_override("margin_left", left)
	n.add_theme_constant_override("margin_top", top)
	n.add_theme_constant_override("margin_right", right)
	n.add_theme_constant_override("margin_bottom", bottom)
	# 竖直方向**默认不**抢多余空间：MarginContainer 大多数时候只是包一层内边距。
	# （早先这里写成 EXPAND_FILL，结果空白的 MarginContainer 会和战场对半分高度。）
	n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	n.size_flags_vertical = Control.SIZE_FILL
	if parent != null:
		parent.add_child(n)
	return n

## 固定尺寸的占位块。**默认不抢空间**（要弹性间隔就自己把 size_flags_horizontal 设成
## EXPAND_FILL）。早先这里默认 EXPAND，导致两列玩家表之间的 10px 分隔条把 Team 2 顶出屏幕。
static func spacer(min_size: Vector2) -> MarginContainer:
	var n := MarginContainer.new()
	n.name = "Spacer"
	n.custom_minimum_size = min_size
	n.size_flags_horizontal = Control.SIZE_FILL
	n.size_flags_vertical = Control.SIZE_FILL
	return n

## 注意：默认**不**开自动换行。开了以后 Label 在水平容器里的最小高度会按"宽度 0"估算，
## 把整行撑到几百像素（真踩过：FxLabel 的 min 高度变成 424，整个 PlayScreen 溢出屏幕）。
## 需要换行的地方显式传 `autowrap = true`。
static func label(text_: String, font_size: int = 0, align: int = HORIZONTAL_ALIGNMENT_LEFT,
		autowrap: bool = false) -> Label:
	var n := Label.new()
	n.text = text_
	n.horizontal_alignment = align
	n.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART if autowrap else TextServer.AUTOWRAP_OFF
	if autowrap:
		n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if font_size > 0:
		n.add_theme_font_size_override("font_size", font_size)
	return n

static func title(text_: String, parent: Node = null) -> Label:
	# 原版的标题都是大写（CAMPAIGN / REPLAYS / PLAYER OPTIONS），字号由 TitleSize.gd 按 宽/8 调
	var n := label(text_, 0, HORIZONTAL_ALIGNMENT_CENTER)
	n.name = "Label"
	n.size_flags_vertical = Control.SIZE_EXPAND_FILL
	n.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	if parent != null:
		parent.add_child(n)
	return n

static func rich(font_size: int = 0) -> RichTextLabel:
	var n := RichTextLabel.new()
	n.bbcode_enabled = true
	n.fit_content = true
	n.scroll_active = false
	n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if font_size > 0:
		n.add_theme_font_size_override("normal_font_size", font_size)
	return n

static func panel(name_: String = "Panel", parent: Node = null) -> PanelContainer:
	var n := PanelContainer.new()
	n.name = name_
	n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	n.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if parent != null:
		parent.add_child(n)
	return n

static func scroll(name_: String = "ScrollContainer") -> ScrollContainer:
	var n := ScrollContainer.new()
	n.name = name_
	n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	n.size_flags_vertical = Control.SIZE_EXPAND_FILL
	n.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	return n

static func separator_line() -> HSeparator:
	return HSeparator.new()

## 广告位。
##
## 原版 11 个界面底部都有 60 px 的 `AdContainer`（`docs/104` §4.3）。
## 用户明确要求 Linux 端没有广告，所以这里保留**同样高度的占位条**，
## 写上 disabled 说明 —— 布局不变，功能不做（`docs/108` §7）。
static func ad_bar() -> MarginContainer:
	var m := MarginContainer.new()
	m.name = "AdContainer"
	m.custom_minimum_size = Vector2(0, FOOTER_AD_HEIGHT)
	m.add_theme_constant_override("margin_left", 8)
	m.add_theme_constant_override("margin_top", 4)
	m.add_theme_constant_override("margin_right", 8)
	m.add_theme_constant_override("margin_bottom", 4)
	var l := label("advertisement slot — disabled in this build", 0, HORIZONTAL_ALIGNMENT_CENTER)
	l.name = "AdPlaceholder"
	# 固定小字号 + 截断：这一条**不能**参与最小尺寸计算，否则界面的最小宽度会被它撑开
	# （实测：把广告条字号跟着屏幕缩放后，Campaign 的最小宽度变成 1735px）。
	l.add_theme_font_size_override("font_size", 14)
	l.clip_text = true
	l.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	l.add_theme_color_override("font_color", GwThemeFactory.COL_TEXT_DIM)
	m.add_child(l)
	return m

## 返回按钮对。原版每个界面都有 `ExitButtonContainer/{ClipButton,LongPressButton}`：
## 观测到"点 ClipButton 不响应、点 LongPressButton 生效"（`docs/92` §4.11），
## 原因是 `ClipButton` 只是动画层。这里保留两个按钮，把命令挂在后者上。
static func exit_pair(cmd: String) -> MarginContainer:
	var container := MarginContainer.new()
	container.name = "ExitButtonContainer"
	var stack := Control.new()
	stack.name = "Stack"
	stack.custom_minimum_size = Vector2(64, 48)
	container.add_child(stack)

	var clip := GwButton.make("", "")
	clip.name = "ClipButton"
	clip.text = "     "
	clip.icon = GwIcons.get_icon("return")
	clip.flat = true
	clip.set_anchors_preset(Control.PRESET_FULL_RECT)
	clip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stack.add_child(clip)

	var anim := ColorRect.new()
	anim.name = "AnimationRect"
	anim.color = Color(1, 1, 1, 0.0)
	anim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	anim.set_anchors_preset(Control.PRESET_FULL_RECT)
	clip.add_child(anim)

	var press := GwButton.make("")
	press.name = "LongPressButton"
	press.text = "     "      # 原版文本是 5 个空格；宽度靠 icon 撑，不靠空格
	press.input_text = cmd
	press.icon = GwIcons.get_icon("return")
	press.set_anchors_preset(Control.PRESET_FULL_RECT)
	stack.add_child(press)
	return container

## 通用的"全屏骨架"：MarginContainer → VBoxContainer。
## 返回 {margin, vbox}，调用方往里塞内容。
static func frame(root: Control) -> Dictionary:
	var m := margin("MarginContainer", 10)
	m.set_anchors_preset(Control.PRESET_FULL_RECT)
	m.name = "MarginContainer"
	root.add_child(m)
	var v := vbox("VBoxContainer", 10, m)
	return {"margin": m, "vbox": v}

## 弹窗骨架：MarginContainer → VBoxContainer → 居中的 Panel。
## 返回 {margin, vbox, panel, content}（content 是要塞内容的 VBox）。
static func dialog(root: Control, panel_min: Vector2 = Vector2(560, 320)) -> Dictionary:
	var f := frame(root)
	var mid := margin("HBoxContainer2", 12, f.vbox)
	mid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var p := panel("Panel", mid)
	p.custom_minimum_size = panel_min
	var content := vbox("VBoxContainer", 10, p)
	return {"frame": f, "margin": f.margin, "vbox": f.vbox, "panel": p, "content": content}

## 模式图标按钮（不可点，仅显示）。原版里这些按钮点下去会循环，但从
## JoinLevelPopUp / game_room_entry / replay_entry 里的同名节点看，它们是"显示当前模式"。
static func mode_button(icon_name: String, tooltip: String = "") -> GwButton:
	var b := GwButton.make("")
	b.name = "ModeButton"
	b.icon = GwIcons.get_icon(icon_name)
	b.custom_minimum_size = Vector2(56, 56)
	b.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	b.disabled = false
	b.tooltip_text = tooltip
	return b
