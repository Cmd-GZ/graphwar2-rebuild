extends GwScreen
## PlayerScreen —— 玩家选项（改名 + Skins/Faces/Hats 三个页签）。
##
## **静态节点树全部来自原版 `.tscn`**：`client/scenes/screens/player_screen.tscn` 与
## `recovered-src/player_screen.tscn` **逐字节相同**（473 行 / 64 节点）。原版结构：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer/{ExitButtonContainer(exit_player_settings), MarginContainer/Label("PLAYER OPTIONS")}
##     ├ HBoxContainer3                       ← 两个子节点都 size_flags_horizontal=3 ⇒ ≈50/50
##     │    ├ TabContainer (PlayerTabContainer.gd → 三个标题，含 U+2009 窄空格)
##     │    │    ├ SkinsContainer/{VBox/HBox/{Margin10, SkinsVbox, Margin10}, …}
##     │    │    │      SkinsVbox(ShopButtonsSize) = 5 行 × 3 个 **白卡按钮**（15）
##     │    │    ├ FacesContainer/…/FacesVbox  = 5 行 × 3 个 shop_button_entry（15）
##     │    │    └ HatsScroll/…/HatsVbox       = 9 行 × 3 个 shop_button_entry（27）
##     │    └ VBoxContainer（右列）
##     │         ├ HBoxContainer2/{MarginContainer/Label(" NAME: "), MarginContainer2/PlayerNameLineEdit}
##     │         └ MarginContainer(ShowcaseMarginContainer)/AspectRatioContainer
##     │              ├ PlayerShowcase        （**原版是 Rust 类**）
##     │              ├ SkinBlock（桌面端原版不显示 ⇒ 本类 hide，见下）
##     │              └ PendingBlock（`.tscn` 里静态 `visible = false`）
##     └ AdContainer（运行 0 高、无文字）
##
## # 本文件还剩什么（**动态**部分，原版在 Rust `player_screen_control` 里）
##
## `.tscn` 里三个网格容器（`SkinsVbox` / `FacesVbox` / `HatsVbox`）是**空的**，格子由
## `GraphwarPlayerScreenControl::load_items`（`0x4f57d0`，T45 已按地址落码）在运行时填。
## 我方那 8 个函数还没接节点层 ⇒ 格子仍由本文件的 `_build_*_page()` 造（**我方仿制**，
## 必须声明）。**造格子这层不删**：删了整屏就是空的（`docs/06` §97 遗留第 1 条）。
##
## 三件有真帧逐像素证据的事（`docs/129`、`docs/145`）：
## 1. **格子是 303×303 的白卡**：`ShopButtonsSize.gd`（原版脚本，`.tscn` 里挂在三个网格上）
##    = `(安全区宽/2 − 50)/3` → 1920 下 303.33。我方 `_rebuild_cell_size()` 仍算一遍
##    （取整到 303）——两者在 917 px 的可用宽里收缩后像素一致，实测见 docs/06 §97。
## 2. **三页签格数 15 / 15 / 27**（Skins 5 行、Faces 5 行、Hats 9 行，每行 3 个）。
## 3. **格子里的图标顺序不是 `GwIcons` 数组的顺序**：`SKIN_GRID` / `FACE_GRID` / `HAT_GRID`
##    是拿真帧 15/15/27 个格子逐个与资产做模板匹配得到的（`docs/129` §2）。
##
## 第 4 件（`docs/145` §1）：**Faces / Hats 两页是"当前皮肤 + 该格图标"的运行时合成**，
## 不是 build 那一刻的快照 ⇒ 换皮肤（或重进本屏）必须把 42 个皮肤层全部重画。
##
## 未实现：解锁/内购（Linux 端不做）、段位、皮肤合法性校验。

const COLS := 3

## 网格 = **栅格里的显示顺序**，元素是 `GwIcons` 数组下标（即 `profile.skin` 的取值）。
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

## 原版 `.tscn` 里的节点路径（一字不改）。
const TABS_PATH := "MarginContainer/VBoxContainer/HBoxContainer3/TabContainer"
const GRID_SKINS := "SkinsContainer/VBoxContainer/HBoxContainer/SkinsVbox"
const GRID_FACES := "FacesContainer/VBoxContainer/HBoxContainer/FacesVbox"
const GRID_HATS := "HatsScroll/VBoxContainer/HBoxContainer/HatsVbox"
const NAME_EDIT_PATH := "MarginContainer/VBoxContainer/HBoxContainer3/VBoxContainer"\
	+ "/HBoxContainer2/MarginContainer2/PlayerNameLineEdit"
const SHOWCASE_PATH := "MarginContainer/VBoxContainer/HBoxContainer3/VBoxContainer"\
	+ "/MarginContainer/AspectRatioContainer/PlayerShowcase"
const SKIN_BLOCK_PATH := "MarginContainer/VBoxContainer/HBoxContainer3/VBoxContainer"\
	+ "/MarginContainer/AspectRatioContainer/SkinBlock"

var _tabs: TabContainer
var _name_edit: LineEdit
var _showcase: Control

## Faces / Hats 两页**每一格的那层皮肤圆**（`shop_button_entry.tscn` 的
## `GraphwarButton/SkinBackgroundButton`）。原版这两页是**动态合成**：
## 「当前 `profile.skin` 的皮肤圆 + 该格的脸/帽子图标」（真帧证据见 `docs/145` §1）。
var _entry_skin_layers: Array[GwButton] = []

## 格子边长（`ShopButtonsSize.gd`）。1920 宽 ⇒ 303.33 → 取整 303。
var _cell := 96.0

func _ready() -> void:
	# 本屏所有文字都是 宽/30（`PlayerTabContainer.gd` / `LanLobbyTitle.gd` /
	# `ExitButtonContainer.gd`），只有 PlayerNameLineEdit 是 宽/40（`IpPopUpMargin.gd`）。
	font_divisor = 30
	use_safe_area = true
	super()

## 只找句柄 + 填**动态**格子；静态骨架一行都不造了（原版 `.tscn` 承载）。
func build() -> void:
	_tabs = get_node_or_null(TABS_PATH) as TabContainer
	if _tabs == null:
		push_error("PlayerScreen: 原版 .tscn 里没有 TabContainer（路径变了？）%s" % TABS_PATH)
		return
	_rebuild_cell_size()
	_build_skins_page()
	_build_faces_page()
	_build_hats_page()
	_build_name_edit()
	_build_showcase()
	# 三个网格上挂的是**原版** `ShopButtonsSize.gd`：它 `_ready` 里 `call_deferred("update_sizes")`，
	# 会把这个 VBox 的所有非 Container 后代（= 我们刚造的格子）`custom_minimum_size` 设成
	# `(安全区宽/2 − 50)/3` —— **没有下限**（headless 64×64 下算出来是 −6）。我们必须在那之后
	# 再写一遍，否则格子边长会被写成负数（`selftest` 的既有断言要求它有 32 的下限，那是
	# **我方口径**，见 `docs/16` §39.6：原版 105 个脚本里 `maxi`/`max(` 出现 0 次）。
	# 延迟注册顺序：ShopButtonsSize 在 `_ready`（子节点先跑）就注册了，本句在父节点的 `_ready`，
	# 一定排在它后面 ⇒ 同一帧里最后写入的是我方那次数值。1920 下两者是 303 vs 303.33。
	call_deferred("_reapply_cell_sizes")
	# `SkinBlock`（"Support the game" 那块 alpha 0.9 黑底）在**桌面端原版不显示**：
	# 1920×1160 真帧的右列下半屏是纯底色（`docs/92` §4.14、`docs/129`）。原版那侧由
	# Rust `player_screen_control` 的 `lock/unlock` 决定显隐（`0x4f8850`/`0x4f8990`，T45 已落码），
	# 节点层还没接 ⇒ 本类先按真帧 hide。
	var skin_block := get_node_or_null(SKIN_BLOCK_PATH) as Control
	if skin_block != null:
		skin_block.visible = false

# ---------------------------------------------------------------- 三个页签（动态格子）

## 填已有的网格容器（`SkinsVbox` / `FacesVbox` / `HatsVbox`，都是从 `.tscn` 里取的）。
func _grid(rel_path: String) -> VBoxContainer:
	var g := get_node_or_null(TABS_PATH + "/" + rel_path) as VBoxContainer
	if g == null:
		push_error("PlayerScreen: 原版 .tscn 里没有网格容器 %s" % rel_path)
	return g

func _rows(count: int) -> int:
	return int(ceil(float(count) / float(COLS)))

func _build_skins_page() -> void:
	var grid := _grid(GRID_SKINS)
	if grid == null:
		return
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
	var grid := _grid(GRID_FACES)
	if grid == null:
		return
	_fill_entry_grid(grid, FACE_GRID, "face", _on_face_selected)

func _build_hats_page() -> void:
	var grid := _grid(GRID_HATS)
	if grid == null:
		return
	_fill_entry_grid(grid, HAT_GRID, "hat", _on_hat_selected)

func _row(grid: VBoxContainer) -> HBoxContainer:
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, grid)
	row.size_flags_vertical = Control.SIZE_FILL
	return row

## Skins 页的格子：**一个**按钮（原版运行时也是往 `SkinsVbox` 里塞
## `GraphwarButton`，不是 `shop_button_entry`）。
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
## ⚠️ 两处关键（`docs/145` §1）：①**皮肤层只有一份**（白卡不带 icon）；②皮肤层的图标必须
## **跟着 `profile.skin` 走**（原版是运行时合成，不是静态值）。
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
func _apply_skin_to_entries(skin_index: int) -> void:
	var icon := GwIcons.get_icon("skin_%d" % skin_index)
	for layer in _entry_skin_layers:
		if is_instance_valid(layer):
			layer.icon = icon

## 白卡之上的一层图标：不吃鼠标、不留底色，只把 512² 画布铺满内容区。
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
## 读不到 App（自检/孤立场景）时退回 `skins/yellow` 的下标 —— 原版无存档时的默认值
## （`docs/137` §5.1）。
func _current_skin_index() -> int:
	var app_node := app()
	if app_node != null:
		return int(app_node.store.profile.skin)
	return 7     # `GwIcons.SKINS[7] == skins/yellow`

# ---------------------------------------------------------------- 右列

## 改名输入框：**节点来自原版 `.tscn`**（`PlayerNameLineEdit` + 原版 `PlayerNameLineEdit.gd`
## 空壳脚本）。我方只接"文本变化 → 存档"这一条控制器逻辑。
func _build_name_edit() -> void:
	_name_edit = get_node_or_null(NAME_EDIT_PATH) as LineEdit
	if _name_edit == null:
		push_error("PlayerScreen: 原版 .tscn 里没有 PlayerNameLineEdit")
		return
	if not _name_edit.text_changed.is_connected(_on_name_changed):
		_name_edit.text_changed.connect(_on_name_changed)

## 名片预览：`.tscn` 里那个节点的类型是 **GDExtension 类 `PlayerShowcase`**（原版就是这个
## 类，见 `src/_scaffold/player_showcase_class.rs`）。但那个类目前只画**三个纯色方块**
## （真贴图来源 = 根节点 `+0x16a8` 的档案 + 纹理加载器，未完全复原），挂上去整块预览就不是
## 真人的三层合成 ⇒ 本轮**换成**我方 `GwPlayerShowcase`（`client/src/play/player_showcase.gd`，
## 用真图标三层合成）。**这是替身，不是复原**，登记在 `docs/06` §97 遗留第 2 条。
func _build_showcase() -> void:
	var native := get_node_or_null(SHOWCASE_PATH) as Control
	var parent: Node = native.get_parent() if native != null else null
	var slot := native.get_index() if native != null else 0
	if native != null:
		parent.remove_child(native)
		native.queue_free()
	_showcase = GwPlayerShowcase.new()
	_showcase.name = "PlayerShowcase"
	if parent != null:
		parent.add_child(_showcase)
		parent.move_child(_showcase, slot)

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
	_reapply_cell_sizes()

## 把格子边长按 `_rebuild_cell_size()` 写一遍（窗口变化 / 延迟补写都走这里）。
func _reapply_cell_sizes() -> void:
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
