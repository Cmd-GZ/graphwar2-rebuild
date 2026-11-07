extends GwScreen
## PlayerScreen —— 玩家选项（改名 + Skins/Faces/Hats 三个页签）。
##
## 结构（`docs/92` §4.14 + 恢复出的 `player_screen.tscn` 64 个静态节点）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer/{ExitButtonContainer(exit_player_settings), MarginContainer/Label("PLAYER OPTIONS")}
##     ├ HBoxContainer3                       ← 两个子节点都 size_flags_horizontal=3 ⇒ ≈50/50
##     │    ├ TabContainer (PlayerTabContainer.gd → 三个标题，含 U+2009 窄空格)
##     │    │    ├ SkinsContainer/{VBox/HBox/{Margin10, SkinsVbox, Margin10}, …}
##     │    │    │      SkinsVbox(ShopButtonsSize) = 5 行 × 3 个 **白卡按钮**（15）
##     │    │    ├ FacesContainer/…/FacesVbox  = 5 行 × 3 个 shop_button_entry（15）
##     │    │    └ HatsScroll/…/HatsVbox       = 9 行 × 3 个 shop_button_entry（27）
##     │    └ VBoxContainer（右列，size_flags_horizontal=3）
##     │         ├ HBoxContainer2/{MarginContainer/Label(" NAME: "), MarginContainer2/PlayerNameLineEdit}
##     │         └ MarginContainer(ShowcaseMarginContainer, margin 5)/AspectRatioContainer
##     │              ├ PlayerShowcase        （Rust 自绘）
##     │              ├ SkinBlock（visible=false，桌面端原版不显示）
##     │              └ PendingBlock（visible=false）
##     └ AdContainer（运行 0 高、无文字）
##
## 三件本轮纠正过来的事（全部有真帧逐像素证据，见 `docs/129`）：
##
## 1. **格子是 303×303 的白卡**，不是 96。96 是 `ButtonGrid.gd` 的公式，那是别的控件用的；
##    玩家页挂的是 `ShopButtonsSize.gd`：`(安全区宽/2 − 50)/3` → 1920 下 303.33。
##    白底黑框来自 `shop_theme.tres`（Skins）与 `shop_button_entry.tscn` 里那个默认白色的
##    `ColorRect`（Faces/Hats），见 `GwThemeFactory.shop_cell_stylebox()`。
## 2. **三页签格数 15 / 15 / 27**。真帧数行：Skins 5 行、Faces 5 行、Hats 9 行，每行 3 个；
##    行数是用**滚动条长度比**（grabber/轨道 = 可视高/内容高）反解的，不是靠"数看得见的行"
##    —— 见 docs/129 §2.1（这一点我第一次数错过，写成 24，后来用滚动条比例纠正回来）。
## 3. **格子里的图标顺序不是 `GwIcons` 数组的顺序**：原版网格跳过了一些"不该出现在商店里"
##    的资源。顺序在下面 `SKIN_GRID` / `FACE_GRID` / `HAT_GRID` 里，每一条都是把真帧的
##    15/15/27 个格子逐个和资产做模板匹配（皮肤比主色、脸/帽子比 alpha 合成）得到的，
##    写在 `docs/129` §2。
##
## 第 4 件（`docs/145` §1）：**Faces / Hats 两页是"当前皮肤 + 该格图标"的运行时合成**，
## 不是 build 那一刻的快照 —— 换皮肤（或重新进入本屏）必须把 42 个皮肤层全部重画；
## 白卡层只画底色（`shop_button_entry.tscn` 里那个根 `GraphwarButton` 没有 `icon=`）。
##
## 未实现：解锁/内购（Linux 端不做，用户要求）、段位、皮肤合法性校验。

const COLS := 3

## 网格 = **栅格里的显示顺序**，元素是 `GwIcons` 数组下标（即 `profile.skin` 的取值）。
## 也就是说 `SKIN_GRID[2] == 3` 表示"第 2 格显示 `GwIcons.SKINS[3]`，点它就把 skin 设成 3"。
##
## Skins（15 格）：跳过 `skins/gray` 与 `skins/target`（原版商店里没有这两张）。
const SKIN_GRID: Array[int] = [0, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]

## Faces（15 格）：跳过 `faces/no_face`（那是 Bot 的脸，全透明）。
const FACE_GRID: Array[int] = [2, 3, 4, 5, 1, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]

## Hats（**27** 格 = 9 行 × 3）：只跳过 `antenna_0`。
const HAT_GRID: Array[int] = [
	0, 1, 7,      9, 10, 2,     3, 4, 11,
	5, 12, 17,    13, 8, 14,    15, 16, 18,
	19, 20, 6,    21, 22, 23,   24, 25, 26,
]

## `PlayerTabContainer.gd` 的三个标题（原文，含 U+2009 窄空格）
const TAB_TITLES := [" \u2009 Skins  \u2009", "  \u2009Faces  ", "  \u2009Hats\u2009  \u2009"]

var _tabs: TabContainer
var _name_edit: LineEdit
var _showcase: GwPlayerShowcase

## Faces / Hats 两页**每一格的那层皮肤圆**（`shop_button_entry.tscn` 的
## `GraphwarButton/SkinBackgroundButton`）。原版这两页是**动态合成**：
## 「当前 `profile.skin` 的皮肤圆 + 该格的脸/帽子图标」（真帧证据见 `docs/145` §1）。
## 我们以前在 `build()` 里把皮肤图标**一次性**写死进格子，之后再选皮肤不会重画
## —— 用户看到的就是"整页都是打开界面那一刻的颜色"。这里保留引用，皮肤一变就重刷。
var _entry_skin_layers: Array[GwButton] = []

## 格子边长（`ShopButtonsSize.gd`）。1920 宽 ⇒ 303.33。
var _cell := 96.0

func _ready() -> void:
	# 本屏所有文字都是 宽/30（`PlayerTabContainer.gd` / `LanLobbyTitle.gd` /
	# `ExitButtonContainer.gd`），只有 PlayerNameLineEdit 是 宽/40（`IpPopUpMargin.gd`）。
	font_divisor = 30
	use_safe_area = true
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox
	build_header(vbox, "PLAYER OPTIONS", GwCommands.EXIT_PLAYER_SETTINGS, true)

	# 外层 HBoxContainer3：TabContainer 与右列**都是** size_flags_horizontal=3 ⇒ 平分。
	# 上一轮我们给 TabContainer 写了 stretch_ratio=2.0，于是它吃掉 2/3 宽度、右列被挤到屏幕外
	# —— 这正是真帧右上角"NAME 输入框跑到 x=1307"的原因（原版 x=1207，docs/129 §1.4）。
	var body := GwUI.hbox("HBoxContainer3", GwUI.SEPARATION, vbox)
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL

	_tabs = TabContainer.new()
	_tabs.name = "TabContainer"
	_tabs.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tabs.clip_tabs = false
	_tabs.add_theme_constant_override("side_margin", 10)   # graphwar_theme.tres

	_rebuild_cell_size()
	_build_skins_page()
	_build_faces_page()
	_build_hats_page()
	for i in TAB_TITLES.size():
		_tabs.set_tab_title(i, String(TAB_TITLES[i]))
	# 原版 `PlayerTabContainer.gd` 自己写三个页签标题（含 U+2009）并按 安全区宽/30 设字号。
	# 它的 `_ready` 要能 `set_tab_title(0..2)`，所以**必须**在三个页签都在、且进树之前挂。
	GwViews.attach(_tabs, "PlayerTabContainer.gd")
	body.add_child(_tabs)

	_build_right_column(body)

	vbox.add_child(GwUI.ad_slot())

# ---------------------------------------------------------------- 三个页签

## 一个页签的骨架（三分页结构一模一样，`player_screen.tscn` 里也是三段复制）：
##   ScrollContainer → VBox(sep 4) → {Margin(0,10), HBox → [Margin(10,0), 网格, Margin(10,0)], Margin(0,10)}
## 返回装格子的那个 VBox。
func _shop_page(page_name: String, grid_name: String) -> VBoxContainer:
	var scroll := ScrollContainer.new()
	scroll.name = page_name
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tabs.add_child(scroll)

	var column := VBoxContainer.new()
	column.name = "VBoxContainer"
	column.add_theme_constant_override("separation", GwUI.SEPARATION)
	scroll.add_child(column)
	column.add_child(GwUI.spacer(Vector2(0, 10)))

	var band := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, column)
	band.add_child(GwUI.spacer(Vector2(10, 0)))
	var grid := VBoxContainer.new()
	grid.name = grid_name
	grid.add_theme_constant_override("separation", GwUI.SEPARATION)
	grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	grid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	band.add_child(grid)
	band.add_child(GwUI.spacer(Vector2(10, 0)))

	column.add_child(GwUI.spacer(Vector2(0, 10)))
	return grid

func _rows(count: int) -> int:
	return int(ceil(float(count) / float(COLS)))

func _build_skins_page() -> void:
	var grid := _shop_page("SkinsContainer", "SkinsVbox")
	var index := 0
	for r in _rows(SKIN_GRID.size()):
		var row := _row(grid)
		for c in COLS:
			if index >= SKIN_GRID.size():
				break
			var skin_index := SKIN_GRID[index]
			var b := _cell_button("GraphwarButton%d" % index, GwIcons.get_icon("skin_%d" % skin_index))
			b.pressed.connect(_on_skin_selected.bind(skin_index))
			row.add_child(b)
			index += 1

func _build_faces_page() -> void:
	var grid := _shop_page("FacesContainer", "FacesVbox")
	_fill_entry_grid(grid, FACE_GRID, "face", _on_face_selected)

func _build_hats_page() -> void:
	var grid := _shop_page("HatsScroll", "HatsVbox")
	_fill_entry_grid(grid, HAT_GRID, "hat", _on_hat_selected)

func _row(grid: VBoxContainer) -> HBoxContainer:
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, grid)
	row.size_flags_vertical = Control.SIZE_FILL
	return row

## Skins 页的格子：**一个**按钮（`player_screen.tscn` 里 SkinsVbox 直接摆 GraphwarButton，
## 不是 shop_button_entry）。
func _cell_button(node_name: String, icon: Texture2D) -> GwButton:
	var b := GwButton.make("")
	b.name = node_name
	b.custom_minimum_size = Vector2(_cell, _cell)
	b.size_flags_horizontal = Control.SIZE_FILL
	b.size_flags_vertical = Control.SIZE_FILL
	var white := GwThemeFactory.shop_cell_stylebox()
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		b.add_theme_stylebox_override(state, white)
	b.icon = icon
	b.expand_icon = true
	b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	return b

## Faces/Hats 页的格子：原版是 `shop_button_entry.tscn` 实例（5 节点）。
##   Control(303×303)                     ← ShopButtonsSize 设 custom_minimum_size
##     ├ GraphwarButton   白卡、可点       ← 白底黑框，等价于那个"默认白色"的 ColorRect
##     │   └ ColorRect（默认白，show_behind_parent）
##     ├ SkinBackgroundButton  皮肤层      ← 透明底 + 3px 黑边，icon = **当前皮肤**
##     └ Button            脸/帽子层       ← 叠在皮肤上
## 三层同矩形、同 512² 画布 ⇒ 合成结果就是真帧里"当前皮肤色圆 + 该格图标"的格子。
##
## ⚠️ 两处关键（`docs/145` §1）：
## 1. **皮肤层只有一份**。`shop_button_entry.tscn` 里那个 `GraphwarButton` 根节点
##    **没有** `icon=`（它的 icon 属性在文件里根本不存在），皮肤只由 `SkinBackgroundButton`
##    画。我们以前把皮肤图标同时写给白卡和皮肤层 ⇒ 同一个圆画两遍，边缘 alpha 合成两次
##    （真帧逐像素在圆边上差一圈，`docs/137` §3.4 记的那个 0.572% 就是它）。
## 2. 皮肤层的图标必须**跟着 `profile.skin` 走**（原版是运行时合成，不是静态值）：
##    `shop_button_entry.tscn` 里的 `skins/yellow.svg` 只是场景的静态初值。
func _fill_entry_grid(grid: VBoxContainer, order: Array[int], kind: String, callback: Callable) -> void:
	var index := 0
	for r in _rows(order.size()):
		var row := _row(grid)
		for c in COLS:
			if index >= order.size():
				break
			var item := order[index]
			var entry := Control.new()
			entry.name = "ShopButtonEntry%d" % index
			entry.custom_minimum_size = Vector2(_cell, _cell)
			entry.size_flags_horizontal = Control.SIZE_FILL
			entry.size_flags_vertical = Control.SIZE_FILL

			# 白卡：只有底色，**不带图标**（图标由下面那层皮肤画；见上面的第 1 条）
			var card := _cell_button("GraphwarButton", null)
			card.set_anchors_preset(Control.PRESET_FULL_RECT)
			card.pressed.connect(callback.bind(item))
			entry.add_child(card)

			var skin_layer := _overlay("SkinBackgroundButton",
					GwIcons.get_icon("skin_%d" % _current_skin_index()))
			entry.add_child(skin_layer)
			_entry_skin_layers.append(skin_layer)

			var item_layer := _overlay("Button", GwIcons.get_icon("%s_%d" % [kind, item]))
			entry.add_child(item_layer)

			row.add_child(entry)
			index += 1

## 把 Faces/Hats 的每一格重画成**当前皮肤**（原版的运行时合成，`docs/145` §1.2）。
## 选皮肤（`_on_skin_selected`）和进界面（`on_enter`）都会调它。
func _apply_skin_to_entries(skin_index: int) -> void:
	var icon := GwIcons.get_icon("skin_%d" % skin_index)
	for layer in _entry_skin_layers:
		if is_instance_valid(layer):
			layer.icon = icon

## 白卡之上的一层图标：不吃鼠标、不留底色，只把 512² 画布铺满内容区。
##
## 内容区 = 303 − 2×3（`StyleBoxFlat` 的 3px 边框）；用空样式盒会多出 6 px，
## 圆就从 139 px 变成 142 px（真帧逐像素能看出 1.2%，docs/129 §3.2）。
func _overlay(node_name: String, icon: Texture2D) -> GwButton:
	var b := GwButton.make("")
	b.name = node_name
	b.custom_minimum_size = Vector2.ZERO
	b.set_anchors_preset(Control.PRESET_FULL_RECT)
	b.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var sb := GwThemeFactory.shop_overlay_stylebox()
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		b.add_theme_stylebox_override(state, sb)
	b.icon = icon
	b.expand_icon = true
	b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	return b

## 当前皮肤下标（= `profile.skin` = `GwIcons.SKINS` 的下标）。
##
## `shop_button_entry.tscn` 里 `SkinBackgroundButton` 的 `icon = skins/yellow.svg` 是**静态初值**；
## 运行时原版把它换成**玩家当前皮肤**（真帧：Skin 页选 `red` 之后，Faces 页 15 格的圆心
## 全部由 `(255,204,0)` 变成 `(255,114,114)`，见 `docs/145` §1.1）。
## 所以这里读存档，而不是钉死 yellow。读不到 App（自检/孤立场景）时退回
## `skins/yellow` 的下标 —— 那是原版无存档时的默认值（`docs/137` §5.1）。
func _current_skin_index() -> int:
	var app_node := app()
	if app_node != null:
		return int(app_node.store.profile.skin)
	return 7     # `GwIcons.SKINS[7] == skins/yellow`

# ---------------------------------------------------------------- 右列

func _build_right_column(body: Node) -> void:
	var right := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, body)
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var name_row := GwUI.hbox("HBoxContainer2", GwUI.SEPARATION, right)
	var name_m := GwUI.margin("MarginContainer", 0)
	# `player_screen.tscn` 里这个 MarginContainer **没有** size_flags ⇒ 只占最小宽度；
	# 旁边那个 `MarginContainer2` 才有 `size_flags_horizontal = 3`（吃满剩下的宽度）。
	# `GwUI.margin()` 默认给 EXPAND_FILL，不改的话标签会和输入框各占一半 → 输入框右移。
	name_m.size_flags_horizontal = Control.SIZE_FILL
	var name_label := GwUI.label(" NAME: ")
	name_label.name = "Label"
	var title_font := GwThemeFactory.title_font()
	if title_font != null:
		name_label.add_theme_font_override("font", title_font)
	name_m.add_child(name_label)
	GwViews.attach(name_m, "LanLobbyTitle.gd")   # 字号 = 宽/30
	name_row.add_child(name_m)

	# `IpPopUpMargin.gd`：四边 margin 10，**子节点字号 = 宽/40**（本屏其余文字是 /30）。
	var edit_m := GwUI.margin("MarginContainer2", 10)
	edit_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_name_edit = LineEdit.new()
	_name_edit.name = "PlayerNameLineEdit"
	GwViews.attach(_name_edit, "PlayerNameLineEdit.gd")   # 原版这个 LineEdit 就是空壳脚本
	_name_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# `player_screen.tscn` 静态值就是 12（原版存档里名字也只有 12 字符上限）。
	_name_edit.max_length = 12
	_name_edit.text_changed.connect(_on_name_changed)
	edit_m.add_child(_name_edit)
	GwViews.attach(edit_m, "IpPopUpMargin.gd")   # 四边 10 + 子节点字号 = 宽/40
	name_row.add_child(edit_m)

	# `ShowcaseMarginContainer.gd`：四边 margin 5。
	var showcase_m := GwUI.margin("MarginContainer", 5)
	showcase_m.size_flags_vertical = Control.SIZE_EXPAND_FILL
	GwViews.attach(showcase_m, "ShowcaseMarginContainer.gd")   # 四边 margin 5
	right.add_child(showcase_m)
	var aspect := AspectRatioContainer.new()
	aspect.name = "AspectRatioContainer"
	aspect.ratio = 1.0
	aspect.stretch_mode = AspectRatioContainer.STRETCH_FIT
	aspect.size_flags_vertical = Control.SIZE_EXPAND_FILL
	showcase_m.add_child(aspect)

	_showcase = GwPlayerShowcase.new()
	_showcase.name = "PlayerShowcase"
	aspect.add_child(_showcase)

	# 这两块在**桌面端的原版里不显示**：1920×1160 真帧的右列下半屏是纯底色，既没有
	# "Support the game" 的白字，也没有 SkinBlock 那个 alpha 0.9 的黑底。节点保留
	# （`docs/92` §4.14 的台账要求结构对得上），但 `visible = false` —— 和 `PendingBlock`
	# 在 `.tscn` 里静态写的 `visible = false` 是同一种表达；不用 modulate 透明，
	# 那样占位仍然参与布局。
	aspect.add_child(_build_skin_block())
	aspect.add_child(_build_pending_block())

func _build_skin_block() -> MarginContainer:
	var block := GwUI.margin("SkinBlock", 0)
	block.visible = false
	var back := ColorRect.new()
	back.name = "ColorRect"
	back.color = Color(0, 0, 0, 0.901961)
	back.set_anchors_preset(Control.PRESET_FULL_RECT)
	block.add_child(back)
	var v := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, block)
	var support := GwUI.label("Support the game", 32, HORIZONTAL_ALIGNMENT_CENTER)
	support.name = "Label2"
	support.size_flags_vertical = Control.SIZE_EXPAND_FILL
	support.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	v.add_child(support)
	var ad_note := GwUI.label("Unlocking skins also removes all ads", 32,
			HORIZONTAL_ALIGNMENT_CENTER)
	ad_note.name = "Label"
	ad_note.size_flags_vertical = Control.SIZE_EXPAND_FILL
	ad_note.size_flags_stretch_ratio = 3.0
	ad_note.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	v.add_child(ad_note)
	var unlock_row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION)
	unlock_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var left_pad := GwUI.margin("MarginContainer", 0, unlock_row)
	left_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left_pad.size_flags_stretch_ratio = 0.3
	var unlock := GwButton.make("Unlock", GwCommands.UNLOCK_SKINS)
	unlock.name = "UnlockButton"
	unlock.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	unlock_row.add_child(unlock)
	var right_pad := GwUI.margin("MarginContainer2", 0, unlock_row)
	right_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right_pad.size_flags_stretch_ratio = 0.3
	# 三个子节点（左垫片 / Unlock / 右垫片）齐了再挂原版 `UnlockButtonSize.gd`（字号 = 宽/30）
	GwViews.attach(unlock_row, "UnlockButtonSize.gd")
	v.add_child(unlock_row)
	var bottom_pad := GwUI.margin("MarginContainer2", 0, v)
	bottom_pad.size_flags_vertical = Control.SIZE_EXPAND_FILL
	bottom_pad.size_flags_stretch_ratio = 0.2
	return block

func _build_pending_block() -> MarginContainer:
	var block := GwUI.margin("PendingBlock", 0)
	block.visible = false
	var back := ColorRect.new()
	back.name = "ColorRect"
	back.color = Color(0, 0, 0, 0.901961)
	back.set_anchors_preset(Control.PRESET_FULL_RECT)
	block.add_child(back)
	var v := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, block)
	var pad_top := GwUI.margin("MarginContainer", 0, v)
	pad_top.size_flags_vertical = Control.SIZE_EXPAND_FILL
	pad_top.size_flags_stretch_ratio = 3.0
	var label := GwUI.label("Pending...", 60, HORIZONTAL_ALIGNMENT_CENTER)
	label.name = "Label"
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	v.add_child(label)
	var pad_bottom := GwUI.margin("MarginContainer2", 0, v)
	pad_bottom.size_flags_vertical = Control.SIZE_EXPAND_FILL
	pad_bottom.size_flags_stretch_ratio = 3.0
	return block

# ---------------------------------------------------------------- 尺寸与刷新

## 格子边长 = `ShopButtonsSize.gd` 的公式，一字不改：
##   `minimum_size = (DisplayServer.get_display_safe_area().size.x / 2 - 50) / 3`
## 1920 宽 ⇒ `(960 − 50) / 3 = 303.33`，**向下取整到 303**：真帧白区 297 + 3px 边框×2 = 303，
## 三列 17..320 / 324..627 / 631..934，间距 4（逐像素对得上）。取整方式见 docs/129 §2.1。
func _rebuild_cell_size() -> void:
	var width := float(DisplayServer.get_display_safe_area().size.x)
	if width <= 0.0:
		width = get_viewport_rect().size.x
	if width <= 0.0:
		_cell = 96.0
		return
	_cell = maxf(32.0, floorf((width / 2.0 - 50.0) / 3.0))

func apply_font_scale() -> void:
	super()
	if not _built:
		return
	_rebuild_cell_size()
	for n in find_children("*", "GwButton", true, false):
		var b := n as GwButton
		if b.get_parent() is Control and (b.get_parent() as Control).name.begins_with("ShopButtonEntry"):
			continue
		if String(b.name).begins_with("GraphwarButton"):
			b.custom_minimum_size = Vector2(_cell, _cell)
	for n in find_children("ShopButtonEntry*", "Control", true, false):
		(n as Control).custom_minimum_size = Vector2(_cell, _cell)
	_showcase.queue_redraw()

func on_enter() -> void:
	var app_node := app()
	if app_node == null:
		return
	var p: Dictionary = app_node.store.profile
	_name_edit.text = String(p.name)
	# Faces/Hats 两页的皮肤圆跟着存档走（原版是运行时合成，不是 build 那一刻的快照）
	_apply_skin_to_entries(int(p.skin))
	_showcase.set_look(int(p.skin), int(p.face), int(p.hat))

func _on_name_changed(new_text: String) -> void:
	var app_node := app()
	if app_node == null:
		return
	app_node.store.profile.name = new_text
	app_node.store.save_all()

func _on_skin_selected(index: int) -> void:
	var app_node := app()
	if app_node == null:
		return
	app_node.store.profile.skin = index
	app_node.store.save_all()
	# **立刻**重画 Faces/Hats 的皮肤圆：原版换了皮肤之后，翻到那两页看到的就是新颜色
	# （真帧证据 `docs/145` §1.1）。
	_apply_skin_to_entries(index)
	_showcase.set_look(index, int(app_node.store.profile.face), int(app_node.store.profile.hat))

func _on_face_selected(index: int) -> void:
	var app_node := app()
	if app_node == null:
		return
	app_node.store.profile.face = index
	app_node.store.save_all()
	_showcase.set_look(int(app_node.store.profile.skin), index, int(app_node.store.profile.hat))

func _on_hat_selected(index: int) -> void:
	var app_node := app()
	if app_node == null:
		return
	app_node.store.profile.hat = index
	app_node.store.save_all()
	_showcase.set_look(int(app_node.store.profile.skin), int(app_node.store.profile.face), index)
