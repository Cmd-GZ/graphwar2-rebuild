extends GwScreen
## MainMenu —— 主菜单。
##
## 结构（`docs/92` §4.1 运行时 + `docs/104` §4 静态，恢复出的 main_menu.tscn 25 个节点）：
##   MarginContainer/VBoxContainer
##     ├ MarginContainer/Label("Graphwar II")          ← 标题，字号 = 宽/8（TitleSize.gd）
##     ├ HBoxContainer
##     │   ├ MarginContainer(0.2 弹性)/MarginContainer/VBoxContainer
##     │   │     ├ PlayButton("Campaign",  input_text="campaign")
##     │   │     └ CharacterButton("Player\n", input_text="player_settings")
##     │   ├ MarginContainer(0.05 弹性)
##     │   └ MarginContainer2/MarginContainer2/VBoxContainer2
##     │         ├ LocalButton("Battle",   input_text="multiplayer")
##     │   ├ ReplayButton("Replays",  input_text="replay")
##     │         └ MarginContainer3/VersionLabel("2.0.60"，右对齐)
##     └ AdContainer(custom_minimum_size=(0,60))
##
## 观测补充：`VersionLabel` 在 `.tscn` 里写的是 "0.0.0"，运行时显示 "2.0.60" ⇒ 版本号由
## Rust 在运行时填（`docs/92` §4.1）。本项目显示自己的版本号，并注明原版版本。

const GAME_TITLE := "Graphwar II"
## 原版运行时显示的版本（观测）。我们不冒充它：显示自己的客户端版本，后面附原版版本。
const UPSTREAM_VERSION := "2.0.60"
const CLIENT_VERSION := "0.1.0"

func _ready() -> void:
	font_divisor = 30        # MainMenuButtonResize.gd: 宽/30
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox

	# --- 标题：字号 = 宽/8（TitleSize.gd，docs/101 §5.2 表中 N=8 只有它一个）---
	var title_margin := GwUI.margin("MarginContainer", 4, vbox)
	title_margin.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var title := GwUI.title(GAME_TITLE, title_margin)
	title.name = "Label"
	title.set_meta("title_font", true)   # 字号由 apply_font_scale() 按"宽/8"单独设

	# --- 4 个按钮：2×2 布局（原版两列，左列 Campaign/Player，右列 Battle/Replays）---
	var row := GwUI.hbox("HBoxContainer", 12, vbox)
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL

	var left_pad := GwUI.margin("MarginContainer7", 4, row)
	left_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left_pad.size_flags_stretch_ratio = 0.2

	var left_col_margin := GwUI.margin("MarginContainer", 4, row)
	left_col_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var left_col := GwUI.vbox("VBoxContainer", 10, left_col_margin)
	var campaign := _menu_button("Campaign", GwCommands.CAMPAIGN, "campaign")
	campaign.name = "PlayButton"
	left_col.add_child(campaign)
	left_col.add_child(GwUI.spacer(Vector2(0, 0)))
	var player := _menu_button("Player\n", GwCommands.PLAYER_SETTINGS, "player_settings")
	player.name = "CharacterButton"
	left_col.add_child(player)

	var gap := GwUI.margin("MarginContainer2", 4, row)
	gap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	gap.size_flags_stretch_ratio = 0.05

	var right_gap := GwUI.margin("MarginContainer3", 4, row)
	right_gap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right_gap.size_flags_stretch_ratio = 0.05

	var right_col_margin := GwUI.margin("MarginContainer2", 4, row)
	right_col_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right_col_margin.name = "MarginContainer2"
	var right_col := GwUI.vbox("VBoxContainer2", 10, right_col_margin)
	var battle := _menu_button("Battle", GwCommands.MULTIPLAYER, "multiplayer")
	battle.name = "LocalButton"
	right_col.add_child(battle)
	right_col.add_child(GwUI.spacer(Vector2(0, 0)))
	var replays := _menu_button("Replays", GwCommands.REPLAY, "replay")
	replays.name = "ReplayButton"
	right_col.add_child(replays)

	# --- 版本号（原版右对齐挂在右列底部；运行时值 2.0.60 由 Rust 填）---
	var ver_margin := GwUI.margin("MarginContainer3", 4, right_col)
	ver_margin.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var ver := GwUI.label(_version_text(), 0, HORIZONTAL_ALIGNMENT_RIGHT)
	ver.name = "VersionLabel"
	ver.size_flags_vertical = Control.SIZE_SHRINK_END
	ver.tooltip_text = ("本客户端版本 %s；原版客户端版本 %s（docs/92 §4.1 观测）"
		% [CLIENT_VERSION, UPSTREAM_VERSION])
	ver_margin.add_child(ver)

	var tail_gap := GwUI.margin("MarginContainer5", 4, row)
	tail_gap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tail_gap.size_flags_stretch_ratio = 0.05
	var tail_pad := GwUI.margin("MarginContainer6", 4, row)
	tail_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tail_pad.size_flags_stretch_ratio = 0.2

	vbox.add_child(GwUI.ad_bar())

func _version_text() -> String:
	if GwCore.kernel_available():
		return "client %s · kernel on" % CLIENT_VERSION
	return "client %s · kernel off" % CLIENT_VERSION

func _menu_button(text_: String, cmd: String, node_name: String) -> GwButton:
	var b := GwButton.make(text_, cmd)
	b.name = node_name
	b.size_flags_vertical = Control.SIZE_EXPAND_FILL
	b.custom_minimum_size = Vector2(0, 64)
	return b

## 标题字号 = 视口宽 / 8（`TitleSize.gd`，docs/101 §5.2 里 N=8 只有它一个），
## 与按钮的 /30 不同，所以单独覆盖一次。
func apply_font_scale() -> void:
	super()
	if not _built:
		return
	var title := find_child("Label", true, false) as Label
	if title != null and title.has_meta("title_font"):
		var width := get_viewport_rect().size.x
		if width > 0.0:
			title.add_theme_font_size_override("font_size", maxi(24, int(width / 8.0)))

func on_enter() -> void:
	# 版本行会随内核是否加载而变化（主菜单每次回来都刷新一下）。
	var ver := find_child("VersionLabel", true, false)
	if ver != null:
		(ver as Label).text = _version_text()
