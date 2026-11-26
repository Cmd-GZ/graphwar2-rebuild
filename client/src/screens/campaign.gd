extends GwScreen
## Campaign —— 战役关卡选择（21 关）。
##
## **节点树来自原版 `.tscn`**：`client/scenes/screens/campaign.tscn` 与
## `recovered-src/campaign.tscn` **逐字节相同**（92 行 / 12 个静态节点 + 运行时 21 个按钮）。
## 我方这层只剩"进屏时刷解锁状态"，`build()` 不再手抄任何节点（docs/06 §92）。
##
## 原版的接线**全在 `.tscn` 里**（每一处都指向 `recovered-src/` 的原文，不是我方接线）：
##   MarginContainer            ← 原版 `MarginContainer.gd`（安全区 margin）
##     VBoxContainer
##       HBoxContainer
##         ExitButtonContainer  ← 原版 `ExitButtonContainer.gd`（字号 宽/30）
##           ClipButton         ← 原生 `GraphwarButton`（`clip_children=2` 的回弹层）
##             AnimationRect    ← `ColorRect`（`show_behind_parent=true`）
##           LongPressButton    ← 原生 `GraphwarButton`，`input_text = "exit_campaign"`
##         MarginContainer      ← 原版 `LanLobbyTitle.gd`（字号 宽/30）
##           Label              ← 文本 "CAMPAIGN"，`title_theme.tres`（Eraser 手写体）
##       Control                ← 原版 `CampaingLevelsButtons.gd` + `campaign_button_theme_2.tres`
##         level_0..level_20      它 `_ready` 里造 **21 个原生 `GraphwarButton`**：
##                                字号 = 宽/15、x 步长 = 宽/8、y 步长 = 高/4、起点 (x/8, y/8)
##       AdContainer            ← 原版 `AdContainer.gd`（广告位，原版也是 0×0）
##
## 21 个数字的**外观**来自原版 `campaign_button_theme_2.tres`（1 点名 + 5 个 StyleBoxEmpty，
## 见 `comments`）：normal/hover/pressed/disabled/focus **全是 StyleBoxEmpty** ⇒ 没有圆角底、
## 没有边框，数字直接浮在深绿背景上；`font_disabled_color` 的 alpha = 0 ⇒ 锁住的关卡数字
## **根本看不见**；没有覆盖 `font_color` ⇒ 白色从上层 app 主题继承下来。
## ⚠️ 那份主题的 2.44 MB 内嵌字形图集被导出器丢掉了 —— 装上的是**我方 shim**
## （`client/scenes/aliases/campaign_button_theme_2.tres`：把内嵌 FontFile 换成
## `assets/EraserRegular.fontdata`，Theme 层 6 个属性逐条照抄原版）。证据档：结构=观测、
## 字形度量=**推断**（见该文件的头注释与 docs/06 §92.3）。

## 关卡的字体除数：原版 `CampaingLevelsButtons.gd` 用 **宽/15**（本屏标题/返回是 宽/30）。
const FONT_DIVISOR_LEVEL_BUTTON := 15

## 造 21 个关卡按钮的那个 `Control`（原版 `.tscn` 里的节点，挂 `CampaingLevelsButtons.gd`）。
var _area: Control
## 21 个关卡按钮：**原生 `GraphwarButton`**（由原版脚本 `_ready` 里 `GraphwarButton.new()` 造）。
var _buttons: Array[Button] = []

func _ready() -> void:
	# 只有 21 个关卡按钮用 宽/15；标题/返回用 宽/30（LanLobbyTitle.gd、ExitButtonContainer.gd）。
	font_divisor = 30
	super()

func build() -> void:
	# 树来自原版 `.tscn`，这里只取句柄。⚠️ 本函数的调用点在父类 `_ready` 里，而子节点的
	# `_ready` 先跑 ⇒ 原版脚本造好的 21 个按钮此时已经挂在 `Control` 下。
	_area = get_node_or_null("MarginContainer/VBoxContainer/Control") as Control
	if _area == null:
		push_error("[gw2] campaign.tscn 里找不到 MarginContainer/VBoxContainer/Control")
		return
	# 这棵子树的字号归原版脚本算（宽/15）。父类 `apply_font_scale()` 只把
	# `res://src/views/` 路径当"原版脚本"（`GwViews.owns`），而 `.tscn` 引用的是
	# `res://CampaingLevelsButtons.gd` 这条别名 ⇒ 认不出来，用 meta 显式告诉它这一层的除数。
	_area.set_meta("font_divisor", FONT_DIVISOR_LEVEL_BUTTON)
	_buttons.clear()
	for c in _area.get_children():
		if c is Button:
			_buttons.append(c as Button)

func on_enter() -> void:
	refresh_unlock_state()

## 解锁状态：**读原版存档** `user://campaign.save`（`level_<n>` = 最后通关的一关）。
## 存档里说 `level_20` ⇒ 21 关全解锁，21 个数字全是白色（用户真机截图就是这个样子）。
##
## 证据档：**推断**。原版"哪一关该 disabled"的判定点在 Rust 侧，我方还没定位到那条指令；
## 这里是由两点反推的：①主题里 `font_disabled_color` 的 alpha = 0（锁住的数字看不见），
## ②存档格式是原版的（`GwStore.read_campaign_progress`）。**不是**观测到的原版行为。
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
		b.disabled = not unlocked
		b.modulate = Color(1, 1, 1, 1)
		if unlocked:
			b.tooltip_text = "Level %d（占位关卡：真实关卡数据未知）" % i
		else:
			b.tooltip_text = "Level %d — locked（原版存档里还没有通关到这一关）" % i
