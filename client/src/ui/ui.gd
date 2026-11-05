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

## 容器分隔（间距）的"原版值"。
##
## 恢复出的 31 个 `.tscn` 里，`VBoxContainer`/`HBoxContainer` **没有一处**写
## `theme_override_constants/separation`，所以它们用的是 Godot 默认主题的 4 px。
## 我们以前各屏各自写 8/10/12（自定值）—— 对齐几何时统一回到 4（docs/124 §2.2）。
const SEPARATION := 4

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

static func title(text_: String, parent: Node = null, use_title_font: bool = false) -> Label:
	# 原版的标题都是大写（CAMPAIGN / REPLAYS / PLAYER OPTIONS），字号由 TitleSize.gd 按 宽/8 调
	var n := label(text_, 0, HORIZONTAL_ALIGNMENT_CENTER)
	n.name = "Label"
	n.size_flags_vertical = Control.SIZE_EXPAND_FILL
	n.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	# 6 个界面的标题挂 title_theme.tres ⇒ EraserRegular（手写体）；其余走 graphwar_theme。
	if use_title_font:
		var f := GwThemeFactory.title_font()
		if f != null:
			n.add_theme_font_override("font", f)
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

## 广告位容器（`AdContainer`）。**零高度、零文字、默认不可见内容**。
##
## 原版 11 个界面底部确实都有一个 `AdContainer`，但它画不画东西由广告插件决定：
## 恢复出的 `AdContainer.gd` 每次 `_ready` 都把 `custom_minimum_size` 设成
## `Ad_holder.ad_size`，而 `Ad_holder.gd` 里 `ad_size` 的初值是 `Vector2i(0, 0)`，
## 只有 Android 广告真的加载成功后才会被 `on_ad_loaded` 改成横幅尺寸。
## ⇒ **桌面/无广告环境里，AdContainer 的运行尺寸就是 0×0，屏幕上什么都没有。**
##
## 我们以前在这里写了一条 `advertisement slot — disabled in this build`：那是**我们自己
## 编的字**（原版没有），还把 60 px 的死高度钉在 11 个界面底部 —— 用户截图里多出来的那条
## 就是它。现在只留节点、不留文字、不占高度（docs/124 §2.4）。
##
## 注意：`.tscn` 里静态写着 `custom_minimum_size = Vector2(0, 60)`（编辑器占位值），
## 运行时被脚本覆盖成 0 —— "静态 60 / 运行 0"不矛盾，我们对齐的是**运行值**。
static func ad_slot() -> MarginContainer:
	var m := MarginContainer.new()
	m.name = "AdContainer"
	m.custom_minimum_size = Vector2(0, 0)
	m.size_flags_vertical = Control.SIZE_FILL
	return m


## 旧名字保留：`play_screen.gd` / `join_level_pop_up.gd` 仍按这个名字调用
## （那两个文件正由另一位 agent 改，这里不动它们）。语义与 `ad_slot()` 完全相同。
static func ad_bar() -> MarginContainer:
	return ad_slot()

## 返回按钮对。原版每个界面都有 `ExitButtonContainer/{ClipButton,LongPressButton}`：
## 观测到"点 ClipButton 不响应、点 LongPressButton 生效"（`docs/92` §4.11），
## 原因是 `ClipButton` 只是动画层。两个按钮**叠在同一个 MarginContainer 里**
## （原版就是这么写的，没有中间层），因此两者同尺寸、后者压在上面。
##
## 尺寸与图标（照抄 `campaign.tscn` 的四个属性）：`text = "     "`（**5 个空格**，不可见但
## 参与最小尺寸计算）、`icon = return.svg`、`icon_alignment = 1`（居中）、`expand_icon = true`。
## 字号由 `ExitButtonContainer.gd` 设成 **屏幕宽 / 30**（`GwScreen.apply_font_scale()` 负责），
## 所以按钮的尺寸随窗口走 —— 以前我们钉死 64×48，箭头被压成 4:3，形状与原版不同
## （docs/124 §2.5）。样式盒照抄 `long_press_button_theme.tres` 的 normal：
## 透明底 + 3 px 黑边 + 圆角 10。
static func exit_pair(cmd: String) -> MarginContainer:
	var container := MarginContainer.new()
	container.name = "ExitButtonContainer"
	# 原版 `ExitButtonContainer.gd` 自己按 **宽/30** 设字号，与本屏其它文字无关
	# （LanLobby 是 /40，但它的返回键仍然是 /30 —— 真机截图实测 146×96，docs/124 §4）。
	container.set_meta("font_divisor", 30)
	var sb := GwThemeFactory._flat(Color(0.6, 0.6, 0.6, 0.0))

	var clip := GwButton.make("     ", "")
	clip.name = "ClipButton"
	clip.icon = GwIcons.get_icon("return")
	clip.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	clip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	clip.add_theme_stylebox_override("normal", sb)
	container.add_child(clip)

	# 原版 `campaign.tscn` 里 AnimationRect 的四个属性：`show_behind_parent = true`、
	# 满矩形、`color = Color(0.101961, 0.231373, 0.176471, 1)`（= #1A3B2D）。
	# 返回按钮那层**深绿底**就是它画的 —— 不是 stylebox（stylebox 的 bg alpha 是 0）。
	# 我们以前把这层写成全透明，于是返回键只剩一圈黑边（用户看到的"形状不同"）。
	var anim := ColorRect.new()
	anim.name = "AnimationRect"
	anim.show_behind_parent = true
	anim.color = GwThemeFactory.COL_PANEL
	anim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	anim.set_anchors_preset(Control.PRESET_FULL_RECT)
	clip.add_child(anim)

	var press := GwButton.make("     ", cmd)
	press.name = "LongPressButton"
	press.icon = GwIcons.get_icon("return")
	press.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	press.add_theme_stylebox_override("normal", sb)
	press.add_theme_stylebox_override("hover", sb)
	press.add_theme_stylebox_override("pressed", sb)
	press.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	container.add_child(press)
	return container

## 通用的"全屏骨架"：MarginContainer → VBoxContainer。
##
## 边距 = 0、分隔 = 4，都是**原版值**：原版每个界面的根都是一个挂 `MarginContainer.gd` 的
## `MarginContainer`，而那个脚本只在"窗口超出屏幕安全区"时才写 margin 常量
## （恢复出的 `MarginContainer.gd::update_sizes` 可逐行读）；桌面最大化下窗口 == 安全区，
## 所以四个 margin 全是 0。分隔同样没有 override ⇒ 默认 4。
## 我们以前用 10/10，属于自定值（docs/124 §2.2）。
static func frame(root: Control, margin_px: int = 0) -> Dictionary:
	var m := margin("MarginContainer", margin_px)
	m.set_anchors_preset(Control.PRESET_FULL_RECT)
	m.name = "MarginContainer"
	root.add_child(m)
	var v := vbox("VBoxContainer", SEPARATION, m)
	return {"margin": m, "vbox": v}

## 弹窗骨架：MarginContainer → VBoxContainer → 居中的 Panel。
## 返回 {margin, vbox, panel, content}（content 是要塞内容的 VBox）。
static func dialog(root: Control, panel_min: Vector2 = Vector2(560, 320)) -> Dictionary:
	var f := frame(root)
	var mid := margin("HBoxContainer2", 12, f.vbox)
	mid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var p := panel("Panel", mid)
	p.custom_minimum_size = panel_min
	# 弹窗**不占满整屏**：按内容的最小尺寸居中（否则内容少的弹窗会被拉成一整块黑板）。
	p.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	p.size_flags_vertical = Control.SIZE_SHRINK_CENTER
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
