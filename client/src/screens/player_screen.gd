extends GwScreen
## PlayerScreen —— 玩家选项（改名 + Skins/Faces/Hats 三个页签）。
##
## 结构（`docs/92` §4.14，恢复出的 player_screen.tscn 64 个静态节点）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer/{ExitButtonContainer(exit_player_settings), MarginContainer/Label("PLAYER OPTIONS")}
##     ├ HBoxContainer3
##     │    ├ TabContainer (PlayerTabContainer.gd → 三个标题，含 U+2009 窄空格)
##     │    │    ├ SkinsContainer/VBox/HBox/SkinsVbox   ← 5 行 × 3 个 **按钮**（15）
##     │    │    ├ FacesContainer/…/FacesVbox           ← 5 行 × 3 个 shop_button_entry（15）
##     │    │    └ HatsScroll/…/HatsVbox                ← 9 行 × 3 个 shop_button_entry（27）
##     │    └ VBoxContainer
##     │         ├ HBoxContainer2/Label(" NAME: ") + PlayerNameLineEdit
##     │         └ MarginContainer/AspectRatioContainer
##     │              ├ PlayerShowcase        （Rust 自绘）
##     │              ├ SkinBlock（"Unlocking skins also removes all ads" /
##     │              │            "Support the game" / UnlockButton("Unlock", unlock_skins)）
##     │              └ PendingBlock（"Pending..."）
##     └ AdContainer(0,60)
##
## 运行时与静态的差（`docs/104` §5.2）：这些格子**是运行时创建的**（141 个 GraphwarButton、
## 42 个 Control、42 个 ColorRect、19 个 HBoxContainer），没有对应 `.gd` ⇒ 原版是 Rust 建的。
## 本项目也运行时建，格子数照抄：15 + 15×3 + 27×3 = 141 个按钮。
##
## 未实现：解锁/内购（Linux 端不做，用户要求）、段位、皮肤合法性校验。

const SKIN_ROWS := 5
const FACE_ROWS := 5
const HAT_ROWS := 9
const COLS := 3

## `PlayerTabContainer.gd` 的三个标题（`docs/101` §4.4 常量表原文，含 U+2009 窄空格）
const TAB_TITLES := [" \u2009 Skins  \u2009", "  \u2009Faces  ", "  \u2009Hats\u2009  \u2009"]

var _tabs: TabContainer
var _name_edit: LineEdit
var _showcase: GwPlayerShowcase
var _skin_box: VBoxContainer
var _face_box: VBoxContainer
var _hat_box: VBoxContainer

func _ready() -> void:
	font_divisor = 30        # PlayerTabContainer.gd 用安全区宽/30 → 见 use_safe_area
	use_safe_area = true
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox
	build_header(vbox, "PLAYER OPTIONS", GwCommands.EXIT_PLAYER_SETTINGS, true)

	var body := GwUI.hbox("HBoxContainer3", GwUI.SEPARATION, vbox)
	# ⚠️ `GwUI.hbox()` 只给水平 EXPAND，竖直是 FILL —— 这里必须手动撑开，
	# 否则 `body` 只有最小高度（= 页签栏 48 px），TabContainer 与网格被压成一条缝
	# （真渲染截图才发现，docs/124 §4）。
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL

	_tabs = TabContainer.new()
	_tabs.name = "TabContainer"
	_tabs.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tabs.size_flags_stretch_ratio = 2.0
	body.add_child(_tabs)

	# --- Skins：15 个按钮，直接排在 SkinsVbox 里（原版不是 shop_button_entry 实例）---
	var skins_scroll := GwUI.scroll("SkinsContainer")
	skins_scroll.name = "SkinsContainer"
	_tabs.add_child(skins_scroll)
	var skins_v := GwUI.vbox("VBoxContainer", 6, skins_scroll)
	skins_v.add_child(GwUI.spacer(Vector2(0, 10)))
	_skin_box = GwUI.vbox("SkinsVbox", 6, skins_v)
	var current_row: HBoxContainer = null
	for i in GwStore.SKIN_COUNT:
		if i % COLS == 0:
			current_row = GwUI.hbox("HBoxContainer", 6, _skin_box)
			current_row.add_child(GwUI.spacer(Vector2(10, 0)))
		var b := GwButton.make("")
		b.name = "SkinButton%d" % i
		b.icon = GwIcons.get_icon("skin_%d" % i)
		b.expand_icon = true
		b.custom_minimum_size = _cell_size()
		b.tooltip_text = "皮肤 %d" % i
		b.pressed.connect(_on_skin_selected.bind(i))
		if current_row != null:
			current_row.add_child(b)
		if i % COLS == COLS - 1 and current_row != null:
			current_row.add_child(GwUI.spacer(Vector2(10, 0)))
	skins_v.add_child(GwUI.spacer(Vector2(0, 10)))

	# --- Faces：15 个 shop_button_entry ---
	var faces_scroll := GwUI.scroll("FacesContainer")
	faces_scroll.name = "FacesContainer"
	_tabs.add_child(faces_scroll)
	var faces_v := GwUI.vbox("VBoxContainer", 6, faces_scroll)
	faces_v.add_child(GwUI.spacer(Vector2(0, 10)))
	_face_box = GwUI.vbox("FacesVbox", 6, faces_v)
	_fill_shop_grid(_face_box, GwStore.FACE_COUNT, FACE_ROWS, "face", _on_face_selected)
	faces_v.add_child(GwUI.spacer(Vector2(0, 10)))

	# --- Hats：27 个 shop_button_entry ---
	var hats_scroll := GwUI.scroll("HatsScroll")
	hats_scroll.name = "HatsScroll"
	_tabs.add_child(hats_scroll)
	var hats_v := GwUI.vbox("VBoxContainer", 6, hats_scroll)
	hats_v.add_child(GwUI.spacer(Vector2(0, 10)))
	_hat_box = GwUI.vbox("HatsVbox", 6, hats_v)
	_fill_shop_grid(_hat_box, GwStore.HAT_COUNT, HAT_ROWS, "hat", _on_hat_selected)
	hats_v.add_child(GwUI.spacer(Vector2(0, 10)))

	for i in TAB_TITLES.size():
		_tabs.set_tab_title(i, String(TAB_TITLES[i]))

	# --- 右列：名字 + 预览 + 内购块 ---
	var right_col := GwUI.vbox("VBoxContainer", 8, body)
	right_col.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var name_row := GwUI.hbox("HBoxContainer2", 6, right_col)
	var name_m := GwUI.margin("MarginContainer", 2, name_row)
	var name_label := GwUI.label(" NAME: ")
	name_label.name = "Label"
	name_m.add_child(name_label)
	var edit_m := GwUI.margin("MarginContainer2", 2, name_row)
	edit_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_name_edit = LineEdit.new()
	_name_edit.name = "PlayerNameLineEdit"
	_name_edit.custom_minimum_size = Vector2(160, 40)
	_name_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_name_edit.max_length = 24
	_name_edit.tooltip_text = "改名。原版这个 LineEdit 是 Rust 类，text_changed→on_text_change（docs/104 §6.3 #6）"
	_name_edit.text_changed.connect(_on_name_changed)
	edit_m.add_child(_name_edit)

	var showcase_m := GwUI.margin("MarginContainer", 2, right_col)
	showcase_m.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var aspect := AspectRatioContainer.new()
	aspect.name = "AspectRatioContainer"
	aspect.ratio = 0.9
	aspect.stretch_mode = AspectRatioContainer.STRETCH_FIT
	aspect.size_flags_vertical = Control.SIZE_EXPAND_FILL
	showcase_m.add_child(aspect)

	# 展示区内部再叠一层：上面是 PlayerShowcase，右下角压一块 SkinBlock（原版也是叠的，
	# 但 SkinBlock 有自己的底色 ColorRect，所以不会糊在一起）。
	var stack := Control.new()
	stack.name = "ShowcaseStack"
	aspect.add_child(stack)

	_showcase = GwPlayerShowcase.new()
	_showcase.name = "PlayerShowcase"
	_showcase.set_anchors_preset(Control.PRESET_FULL_RECT)
	_showcase.offset_bottom = -150.0        # 底部留给 SkinBlock
	stack.add_child(_showcase)

	var skin_panel := PanelContainer.new()
	skin_panel.name = "SkinBlock"
	skin_panel.anchor_left = 0.0
	skin_panel.anchor_top = 1.0
	skin_panel.anchor_right = 1.0
	skin_panel.anchor_bottom = 1.0
	skin_panel.offset_top = -146.0
	skin_panel.offset_bottom = -4.0
	skin_panel.offset_left = 4.0
	skin_panel.offset_right = -4.0
	skin_panel.size_flags_vertical = Control.SIZE_SHRINK_END
	stack.add_child(skin_panel)
	var skin_block := GwUI.margin("SkinBlockInner", 6, skin_panel)
	var block_v := GwUI.vbox("VBoxContainer", 4, skin_block)
	var support := GwUI.label("Support the game", 0, HORIZONTAL_ALIGNMENT_CENTER)
	support.name = "Label2"
	block_v.add_child(support)
	var ad_note := GwUI.label("Unlocking skins also removes all ads", 0, HORIZONTAL_ALIGNMENT_CENTER)
	ad_note.name = "Label"
	block_v.add_child(ad_note)
	var unlock_row := GwUI.hbox("HBoxContainer", 4, block_v)
	var unlock := GwButton.make("Unlock", GwCommands.UNLOCK_SKINS)
	unlock.name = "UnlockButton"
	unlock.custom_minimum_size = Vector2(120, 40)
	unlock_row.add_child(unlock)

	# PendingBlock（原版是内购挂起遮罩）
	var pending := GwUI.margin("PendingBlock", 4, aspect)
	pending.visible = false
	var pending_v := GwUI.vbox("VBoxContainer", 4, pending)
	var pending_label := GwUI.label("Pending...", 0, HORIZONTAL_ALIGNMENT_CENTER)
	pending_label.name = "Label"
	pending_v.add_child(pending_label)

	vbox.add_child(GwUI.ad_bar())

## 一行 3 个 `shop_button_entry` 等价物：Control{GraphwarButton, SkinBackgroundButton, Button}
## （原版这个子场景恰好 5 个节点，`docs/104` §4.1）。
## 格子尺寸：**原版 `ButtonGrid.gd` 的公式**，一字不改 ——
##   `desired_size = Vector2(screen.x * 0.15 / 3.0, screen.x * 0.15 / 3.0)`
## 1920 宽 ⇒ 96×96。我们以前随手写 72/76，于是整个网格比原版小一圈（真机截图对比，
## docs/124 §4：原版是 3 列 × 96 px 的彩色圆点网格）。
func _cell_size() -> Vector2:
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		return Vector2(96, 96)
	var d := maxf(32.0, w * 0.15 / 3.0)
	return Vector2(d, d)

func apply_font_scale() -> void:
	super()
	if not _built:
		return
	var cell := _cell_size()
	for n in find_children("*", "GwButton", true, false):
		var b := n as GwButton
		if String(b.name).begins_with("ShopButtonEntry") or String(b.name).begins_with("Skin"):
			b.custom_minimum_size = cell

func _fill_shop_grid(box: VBoxContainer, count: int, rows: int, kind: String, callback: Callable) -> void:
	var index := 0
	for r in rows:
		var row := GwUI.hbox("HBoxContainer", 4, box)
		row.add_child(GwUI.spacer(Vector2(10, 0)))
		for c in COLS:
			if index >= count:
				break
			var entry := Control.new()
			entry.name = "ShopButtonEntry%d" % index
			entry.custom_minimum_size = _cell_size()
			entry.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			var back := ColorRect.new()
			back.name = "ColorRect"
			back.color = Color(1, 1, 1, 0.04)
			back.set_anchors_preset(Control.PRESET_FULL_RECT)
			back.mouse_filter = Control.MOUSE_FILTER_IGNORE
			entry.add_child(back)
			var button := GwButton.make("")
			button.name = "GraphwarButton"
			button.icon = GwIcons.get_icon("%s_%d" % [kind, index])
			button.expand_icon = true
			button.set_anchors_preset(Control.PRESET_FULL_RECT)
			button.tooltip_text = "%s %d" % [kind, index]
			button.pressed.connect(callback.bind(index))
			entry.add_child(button)
			# `SkinBackgroundButton`：观测到它挂在 shop_button_entry 里（docs/92 §4.14），
			# 用途未证实（推测是"已选中"底色）。这里只保留节点与角色，不做视觉。
			var bg := GwButton.make("")
			bg.name = "SkinBackgroundButton"
			bg.icon = GwIcons.get_icon("skin_%d" % (index % GwStore.SKIN_COUNT))
			bg.flat = true
			bg.disabled = true
			bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
			bg.visible = false
			entry.add_child(bg)
			# 原版 `shop_button_entry.tscn` 的第 3 个节点也叫 `Button`（类型仍是 GraphwarButton），
			# 5 个节点是 Control + 3×GraphwarButton + ColorRect（docs/104 §4.1）。
			# 它的用途未证实（推测是"价格/锁定"层），这里保留占位。
			var third := GwButton.make("")
			third.name = "Button"
			third.flat = true
			third.disabled = true
			third.visible = false
			third.mouse_filter = Control.MOUSE_FILTER_IGNORE
			entry.add_child(third)
			row.add_child(entry)
			index += 1
		row.add_child(GwUI.spacer(Vector2(10, 0)))

func on_enter() -> void:
	var app_node := app()
	if app_node == null:
		return
	var p: Dictionary = app_node.store.profile
	_name_edit.text = String(p.name)
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
