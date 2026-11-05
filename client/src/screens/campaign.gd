extends GwScreen
## Campaign —— 战役关卡选择（21 关）。
##
## 结构（恢复出的 `campaign.tscn` 12 个静态节点，逐节点照抄）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer
##     │   ├ ExitButtonContainer/{ClipButton, LongPressButton}   ← 左上角返回箭头
##     │   └ MarginContainer/Label("CAMPAIGN")                   ← 字号 = 宽/30（LanLobbyTitle.gd）
##     ├ Control          ← theme = campaign_button_theme_2.tres（关键！见下）
##     ├ 21 个按钮       ← 运行时由 CampaingLevelsButtons.gd 造
##     └ AdContainer      ← 运行时 0×0，没有文字
##
## 21 个按钮的**几何**（`CampaingLevelsButtons.gd`，真源码）：
##   字号 = 容器宽 / 15；x 步长 = 宽/8；y 步长 = 高/4；起点 (x/8, y/8)
##   每次只写 `size.x`，位置再用 `pos - size/2` 居中；x 越过 9/10 或落到 1/10 以下就反向换行。
##
## 21 个数字的**外观**（`campaign_button_theme_2.tres`，逐属性照抄进
## `GwThemeFactory.campaign_level_theme()`）：
##   * 手写体 `Eraser`（font_name 是明写的）—— 和标题 logo 同一套；
##   * normal/hover/pressed/disabled/focus **五个 stylebox 全是 StyleBoxEmpty**
##     ⇒ 没有圆角矩形底、没有边框，数字直接浮在深绿背景上；
##   * `font_disabled_color` 的 alpha = 0 ⇒ 锁住的关卡数字**根本看不见**；
##   * 没有覆盖 `font_color` ⇒ 白色从上层 app 主题继承下来。
## 旧实现给每格画了灰色圆角按钮 + 我们自己的无衬线粗体，那是自造的（docs/124 §3）。

const FONT_DIVISOR_LEVEL_BUTTON := 15

var _area: Control
var _buttons: Array[GwButton] = []
var _level_theme: Theme

func _ready() -> void:
	# 只有 21 个关卡按钮用 宽/15；标题/返回用 宽/30（LanLobbyTitle.gd、ExitButtonContainer.gd）。
	font_divisor = 30
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox
	# 标题字体：campaign.tscn 的 Label 挂的是 title_theme.tres（EraserRegular）。
	build_header(vbox, "CAMPAIGN", GwCommands.EXIT_CAMPAIGN, true)

	_area = Control.new()
	_area.name = "Control"
	_area.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_area.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_area.clip_contents = false
	# 主题挂在容器上（原版就是这么挂的），21 个按钮运行时造出来时自动继承。
	_level_theme = GwThemeFactory.campaign_level_theme()
	_area.theme = _level_theme
	_area.resized.connect(_on_area_resized)
	vbox.add_child(_area)

	for i in GwCampaign.LEVEL_COUNT:
		var level := GwCampaign.level(i)
		var b := GwButton.make(str(i), GwCommands.LEVEL_PREFIX + str(i))
		b.name = "level_%d" % i
		b.text = str(i)
		b.focus_mode = Control.FOCUS_NONE
		b.tooltip_text = "%s（占位关卡，数据未知）" % String(level.title)
		_area.add_child(b)
		_buttons.append(b)

	vbox.add_child(GwUI.ad_slot())
	call_deferred("place_buttons")

func _on_area_resized() -> void:
	# 原版：`self.resized.connect(_on_size_changed)` → `call_deferred("place_buttons")`
	call_deferred("place_buttons")

func on_enter() -> void:
	refresh_unlock_state()
	call_deferred("place_buttons")

## 解锁状态：**读原版存档** `user://campaign.save`（`level_<n>` = 最后通关的一关）。
## 存档里说 `level_20` ⇒ 21 关全解锁，21 个数字全是白色（用户真机截图就是这个样子）。
func refresh_unlock_state() -> void:
	var app_node := app()
	if app_node != null and app_node.store != null:
		app_node.store.campaign_progress = app_node.store.read_campaign_progress()
	for i in _buttons.size():
		var b := _buttons[i]
		var unlocked := true
		if app_node != null and app_node.store != null:
			unlocked = app_node.store.is_level_unlocked(i)
		# 锁定 ⇒ disabled ⇒ 走主题里的 font_disabled_color（alpha = 0）⇒ 数字不可见。
		# 这是我们照主题推出来的行为（原版没有被观测过），docs/124 §3 标为推断。
		b.disabled = not unlocked
		b.modulate = Color(1, 1, 1, 1)
		if unlocked:
			b.tooltip_text = "Level %d（占位关卡：真实关卡数据未知）" % i
		else:
			b.tooltip_text = "Level %d — locked（原版存档里还没有通关到这一关）" % i

## 位置与字号：照抄 `CampaingLevelsButtons.gd::place_buttons`。
func place_buttons() -> void:
	if _area == null or _buttons.is_empty():
		return
	var area := _area.size
	if area.x <= 1.0 or area.y <= 1.0:
		return
	var layout := GwCampaign.layout_buttons(area)
	for i in _buttons.size():
		var item: Dictionary = layout[i]
		var b := _buttons[i]
		b.add_theme_font_size_override("font_size", maxi(10, int(float(item.font_size))))
		# 原版只写 `size.x`，高度交给最小尺寸（= 字号决定的行高）。
		b.size.x = float(item.size_x)
		var h := b.get_combined_minimum_size().y
		var anchor: Vector2 = item.anchor
		b.position = anchor - Vector2(b.size.x, h) / 2.0
		b.size.y = h

## 字号：先按基类把整屏设成 宽/30，再把 21 个关卡数字单独改回 宽/15。
func apply_font_scale() -> void:
	super()
	if not _built:
		return
	_apply_level_font_sizes()

func _apply_level_font_sizes() -> void:
	if _area == null or _buttons.is_empty():
		return
	var width := _area.size.x if _area.size.x > 0.0 else get_viewport_rect().size.x
	if width <= 0.0:
		return
	var size := maxi(10, int(width / float(FONT_DIVISOR_LEVEL_BUTTON)))
	for b in _buttons:
		b.add_theme_font_size_override("font_size", size)
