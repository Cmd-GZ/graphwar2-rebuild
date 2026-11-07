extends GwScreen
## EndGamePopUp —— 结算。
##
## 本轮（docs/130）照恢复出的 `endgame_pop_up.tscn` 逐节点重写，并用"原版 pck 直接
## 实例化 + 同版本引擎"的逐像素真值核对。1920×1160 下量到的矩形（绝对像素）：
##
##   弹窗根   (1,1) 1920×1160    ← 原版根的四个 offset 都是 **1.0**，整屏右/下偏 1 px（照抄）
##   面板     (483,235) 956×691  ← 三列 1:2:1；竖直三条带 **1:3:1**（230 / 691 / 231）
##   返回键   (1,1) 146×96      ← 挂在根节点上的浮层
##
## 原版结构与三个"和别处不一样"的地方：
##   * **没有 AdContainer**（VBox 只有三条带 ⇒ 分隔只有 2 个 4 px，比例才正好 230/691/231）；
##   * `SelfVictoryLabel` 是**静态** `font_size = 70` + `title_theme.tres`（Eraser 手写体），
##     `TeamVictoryLabel` 是静态 `font_size = 40`；两个都**不**跟窗口缩放
##     —— 而 `GwScreen.apply_font_scale()` 会把每个 Label 改成 宽/30，所以这里要盖回去；
##   * 十个 `VictoryPlayerContainerN` 静态是 `visible = false`（真帧里那一行是空的，
##     HBox 会把 543 px 全给它），并且 `HBoxVictoryIcon.gd` 把行内 `separation` 设成
##     **-屏幕宽/10 = -192 px**（图标互相叠着画），不是我们以前的正间距 + 48 px 格子。
##
## 只有真的有人全灭才会出现（`docs/92` §4.13：档 5 的 Bot 31.7 s 后自然出现；
## 等时间不会出现）。本项目由 `App.report_match_result()` 触发。
##
## ⚠️ 验收边界：**"填满图标之后长什么样"没有真值**（没法把正版打到结算），
## 所以这一屏只对齐了"空结算框"的几何（面板/两条文案/返回键）与十个格子的属性。

const VICTORY_SLOTS := 10
## 原版 `endgame_pop_up.tscn` 里两个 Label 的**静态**字号（逐字照抄，不参与缩放）。
const SELF_FONT_SIZE := 70
const TEAM_FONT_SIZE := 40
## 有人的格子占行高的多少（**推断**，见文件头"验收边界"）。
const SLOT_ROW_FRACTION := 0.5

var _self_label: Label
var _team_label: Label
var _icons_row: HBoxContainer
var _slots: Array[AspectRatioContainer] = []

func _ready() -> void:
	font_divisor = 30
	super()
	# 原版根节点的四个 offset 都是 1.0（`anchors_preset = 15` + offset 1）
	offset_left = 1.0
	offset_top = 1.0
	offset_right = 1.0
	offset_bottom = 1.0
	_apply_fixed_metrics()
	var root := get_tree().root
	if root != null and not root.size_changed.is_connected(_apply_fixed_metrics):
		# 在 `GwScreen._install_font_scaling()` 之后连，保证自己**后**执行 ⇒ 盖得住
		root.size_changed.connect(_apply_fixed_metrics)

func build() -> void:
	var frame := GwUI.frame(self)
	var bands: VBoxContainer = frame.vbox

	# 三条竖带，比例 1 : 3 : 1（原版 `HBoxContainer2` 的 `stretch_ratio = 3.0`）
	var top_band := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, bands)
	top_band.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var panel_band := GwUI.hbox("HBoxContainer2", GwUI.SEPARATION, bands)
	panel_band.size_flags_vertical = Control.SIZE_EXPAND_FILL
	panel_band.size_flags_stretch_ratio = 3.0
	var bottom_band := GwUI.hbox("HBoxContainer3", GwUI.SEPARATION, bands)
	bottom_band.size_flags_vertical = Control.SIZE_EXPAND_FILL
	# 原版**没有** AdContainer

	_expander(panel_band, "MarginContainer", 1.0)
	var panel_col := _expander(panel_band, "MarginContainer2", 2.0)
	_expander(panel_band, "MarginContainer3", 1.0)

	var panel := Panel.new()
	panel.name = "Panel"
	panel_col.add_child(panel)
	var content := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, panel_col)

	_self_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_self_label.name = "SelfVictoryLabel"
	var title_font := GwThemeFactory.title_font()
	if title_font != null:
		_self_label.add_theme_font_override("font", title_font)
	content.add_child(_self_label)

	# 图标行：`alignment = 1`（居中）+ `[v=3]`；空格子静态 `visible = false`
	_icons_row = GwUI.hbox("HBoxContainer", GwUI.SEPARATION)
	_icons_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_icons_row.alignment = BoxContainer.ALIGNMENT_CENTER
	for i in VICTORY_SLOTS:
		_slots.append(_victory_slot(_icons_row, i))
	# 原版 `endgame_pop_up.tscn` 这一行挂 `HBoxVictoryIcon.gd`：separation = −视口宽/10
	# （胜利者头像互相叠进去）。五个格子都在了再挂、最后进树。
	GwViews.attach(_icons_row, "HBoxVictoryIcon.gd")
	content.add_child(_icons_row)

	_team_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_team_label.name = "TeamVictoryLabel"
	content.add_child(_team_label)

	var overlay := GwUI.margin("MarginContainer2", 0)
	overlay.add_child(GwUI.exit_pair(GwCommands.EXIT_PLAY))
	GwViews.attach(overlay, "MarginContainerPopUpBack.gd")   # 安全区；桌面下 margin 全 0
	add_child(overlay)

## 一个胜利者格子：`AspectRatioContainer`（`stretch_mode = 1` = 高度控宽度）
## → `MarginContainer` → skin / face / hat 三层 `TextureRect`。
func _victory_slot(parent: HBoxContainer, index: int) -> AspectRatioContainer:
	var ar := AspectRatioContainer.new()
	ar.name = "VictoryPlayerContainer%d" % (index + 1)
	ar.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	ar.stretch_mode = AspectRatioContainer.STRETCH_HEIGHT_CONTROLS_WIDTH
	ar.visible = false                     # 原版 `.tscn` 里就是 false，运行时有数据才显示
	parent.add_child(ar)

	var m := GwUI.margin("MarginContainer", 0, ar)
	var skin := _slot_texture(m, "SkinIcon", GwIcons.get_icon("bot_3"))
	skin.name = "SkinIcon"
	_slot_texture(m, "FaceIcon", null)
	_slot_texture(m, "HatIcon", null)
	return ar

func _slot_texture(parent: MarginContainer, node_name: String, tex: Texture2D) -> TextureRect:
	var t := TextureRect.new()
	t.name = node_name
	t.texture = tex
	# 原版：`expand_mode = 3`（FIT_WIDTH_PROPORTIONAL），stretch_mode 用默认值
	t.expand_mode = TextureRect.EXPAND_FIT_WIDTH_PROPORTIONAL
	parent.add_child(t)
	return t

## 原版两个 Label 的字号是静态 override，图标行的 `separation` 是 `-屏幕宽/10`
## （`HBoxVictoryIcon.gd`）。`GwScreen.apply_font_scale()` 会把 Label 改成 宽/30，
## 所以这里在它之后再写一遍；宽度变化时也要重算 separation。
func _apply_fixed_metrics() -> void:
	if _self_label != null:
		_self_label.add_theme_font_size_override("font_size", SELF_FONT_SIZE)
	if _team_label != null:
		_team_label.add_theme_font_size_override("font_size", TEAM_FONT_SIZE)
	if _icons_row != null:
		_icons_row.add_theme_constant_override("separation",
			-int(get_viewport_rect().size.x / 10.0))

# ---------------------------------------------------------------- 原版骨架的小工具

## 只按宽度分比例的空 `MarginContainer`（原版左右留白，竖直 FILL）。
func _expander(parent: HBoxContainer, node_name: String, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	return m

# ---------------------------------------------------------------- 数据

func on_enter() -> void:
	_apply_fixed_metrics()
	var app_node := app()
	if app_node == null:
		return
	var summary: Dictionary = app_node.match_summary()
	var human_won := bool(summary.get("human_won", false))
	var winner_team := int(summary.get("winner_team", 0))
	var players: Array = summary.players

	# 原版只观测到失败一侧的文案 "YOU lOST"（小写 l，docs/92 §4.13）。
	# 胜利一侧的原文**没有观测**，所以赢的时候用一句明确标注的说明，不编造原文。
	if human_won:
		_self_label.text = "YOU WON"
		_self_label.add_theme_color_override("font_color", GwThemeFactory.COL_ACCENT)
		_self_label.tooltip_text = ("胜利一侧的原文未观测（只观测到 \"YOU lOST\"，docs/92 §4.13）；"
			+ "这里用英文占位并明确标注")
	else:
		_self_label.text = "YOU lOST"    # 原版原文，含它的小写 l
		_self_label.add_theme_color_override("font_color", GwThemeFactory.COL_BAD)
		_self_label.tooltip_text = "原版原文如此（docs/92 §4.13：小写 l）"

	_team_label.text = "Team %d Victory!" % (winner_team + 1)

	var winners: Array = []
	for p in players:
		if int(p.team) == winner_team:
			for i in int(p.soldiers):
				winners.append(p)
	for i in _slots.size():
		var ar := _slots[i]
		var m := ar.get_child(0) as MarginContainer
		var skin_icon := m.get_node_or_null("SkinIcon") as TextureRect
		var face_icon := m.get_node_or_null("FaceIcon") as TextureRect
		var hat_icon := m.get_node_or_null("HatIcon") as TextureRect
		if i < winners.size():
			ar.visible = true
			# ⚠️ 推断：原版运行时给这 10 个格子定多大**没有观测**（`.tscn` 里没有
			# `custom_minimum_size`，而 `stretch_mode = 1` 会让唯一显示的格子撑到行高
			# 453~543 px）。这里取"行高的一半"作正方形：1920×1160 下 = 271 px，
			# 正好接近 HBox 按 `separation = -192` 分给 10 个格子的 268 px（见文件头）。
			var row_h := _icons_row.size.y
			var side := maxf(64.0, row_h * SLOT_ROW_FRACTION)
			ar.custom_minimum_size = Vector2(side, side)
			var p: Dictionary = winners[i]
			var skin_idx := int(p.skin)
			if skin_idx >= 0:
				skin_icon.texture = GwIcons.get_icon("skin_%d" % skin_idx)
			else:
				skin_icon.texture = GwIcons.get_icon("bot_%d" % int(p.bot_tier))
			# 观测：Bot 的脸/帽子分别是 faces/no_face.svg、hats/no_hat.svg（这里留空 = 无脸/无帽）
			face_icon.texture = GwIcons.get_icon("face_%d" % int(p.face)) if int(p.face) >= 0 else null
			hat_icon.texture = GwIcons.get_icon("hat_%d" % int(p.hat)) if int(p.hat) >= 0 else null
			face_icon.visible = int(p.face) >= 0
			hat_icon.visible = int(p.hat) >= 0
		else:
			ar.visible = false
			ar.custom_minimum_size = Vector2.ZERO
			skin_icon.texture = null
			face_icon.visible = false
			hat_icon.visible = false
