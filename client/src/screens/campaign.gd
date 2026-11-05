extends GwScreen
## Campaign —— 战役关卡选择（21 关）。
##
## 结构（`docs/92` §4.2 / `docs/104` §5.3 / 恢复出的 campaign.tscn 12 个静态节点）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer/{ExitButtonContainer, MarginContainer/Label("CAMPAIGN")}
##     ├ Control                    ← 21 个按钮在**运行时**造出来（静态场景里没有）
##     └ AdContainer(0,60)
##
## 21 个按钮由 `CampaingLevelsButtons.gd` 生成（`docs/101` §4.1 有真源码）：
##   `GraphwarButton.new()` / `text = str(i)` / `input_text = "level_" + str(i)` /
##   `set_name("level_" + str(i))` / 字号 = 容器宽/15 / 8 列 4 行 / 碰边回弹。
## 本项目照抄这套算法（`campaign.gd::layout_buttons`），只把 `GraphwarButton` 换成 `GwButton`。

const FONT_DIVISOR_LEVEL_BUTTON := 15

var _area: Control
var _buttons: Array[GwButton] = []

func _ready() -> void:
	# CampaingButtonSize.gd 的 宽/15 **只作用于 21 个关卡按钮**（在 place_buttons 里单独设），
	# 界面其它文字用常规字号。早先把整屏都按 /15 缩放，字号 114px 把广告条撑成 1715px 宽。
	font_divisor = 30
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox
	build_header(vbox, "CAMPAIGN", GwCommands.EXIT_CAMPAIGN)

	_area = Control.new()
	_area.name = "Control"
	_area.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_area.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_area.clip_contents = false
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

	vbox.add_child(GwUI.ad_bar())
	call_deferred("place_buttons")

func _on_area_resized() -> void:
	# 原版：`self.resized.connect(_on_size_changed)` → `call_deferred("place_buttons")`
	call_deferred("place_buttons")

func on_enter() -> void:
	refresh_unlock_state()
	call_deferred("place_buttons")

## 锁定态：**本项目自定规则**（第 0 关可玩，其余要求上一关通关）。
## 原版的锁定判据未观测（docs/92 §8），所以这里明确标成我们自己的规则。
func refresh_unlock_state() -> void:
	var app_node := app()
	var completed: Array = []
	if app_node != null:
		completed = app_node.store.completed_levels
	for i in _buttons.size():
		var unlocked := GwCampaign.is_unlocked(i, completed)
		var b := _buttons[i]
		b.disabled = not unlocked
		var done := completed.has(i)
		b.modulate = Color(1, 1, 1, 1) if unlocked else Color(1, 1, 1, 0.45)
		if done:
			b.tooltip_text = "Level %d — cleared" % i
		elif unlocked:
			b.tooltip_text = "Level %d（占位关卡：真实关卡数据未知）" % i
		else:
			b.tooltip_text = "Level %d — locked（本项目规则：先通关上一关）" % i

## 位置与字号：完全照抄 `CampaingLevelsButtons.gd::place_buttons` 的算法。
func place_buttons() -> void:
	if _area == null or _buttons.is_empty():
		return
	var area := _area.size
	if area.x <= 1.0 or area.y <= 1.0:
		return
	var layout := GwCampaign.layout_buttons(area)
	if OS.get_environment("GW2_DEBUG_LAYOUT") != "":
		print("CAMPAIGN area=%s font=%.1f step=%.1f first3=%s" % [str(area), area.x / 15.0, area.x / 8.0,
			[layout[0].position, layout[1].position, layout[2].position]])
	for i in _buttons.size():
		var item: Dictionary = layout[i]
		var b := _buttons[i]
		var font_size := maxi(10, int(float(item.font_size)))
		b.add_theme_font_size_override("font_size", font_size)
		var h := minf(area.y / 4.5, 56.0)
		b.size = Vector2(float(item.size_x), h)
		b.clip_text = true
		b.position = item.position
