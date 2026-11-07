extends GwScreen
## MainMenu —— 主菜单。**节点树、比例、字号全部照抄恢复出的 `main_menu.tscn`**（25 个节点）。
##
##   MarginContainer                     ← 挂 MarginContainer.gd（安全区；桌面下 margin=0）
##   └ VBoxContainer                     ← separation = 4（Godot 默认，原版没有 override）
##      ├ MarginContainer                [size_flags_vertical=3]  ← 挂 TitleSize.gd
##      │  └ Label "Graphwar II"         ← 字号 = 视口宽/8，EraserRegular（title_theme.tres）
##      ├ HBoxContainer                  [size_flags_vertical=3]
##      │  ├ MarginContainer7            [h=3, stretch_ratio=0.2]   ← 左侧留白
##      │  ├ MarginContainer             [h=3]                     ← 左列
##      │  │  └ VBoxContainer            ← 挂 MainMenuButtonResize.gd（字号 = 视口宽/30）
##      │  │     ├ PlayButton            "Campaign"  input_text="campaign"
##      │  │     ├ MarginContainer       [v=3, ratio=0.05]         ← 按钮之间的弹性空隙
##      │  │     ├ CharacterButton       "Player\n"  input_text="player_settings"
##      │  │     └ MarginContainer2      [v=3]                     ← 底部弹性空隙
##      │  ├ MarginContainer3            [h=3, ratio=0.05]         ← 两列之间
##      │  ├ MarginContainer2            [h=3]                     ← 右列
##      │  │  └ VBoxContainer2           ← 同样挂 MainMenuButtonResize.gd
##      │  │     ├ LocalButton           "Battle"    input_text="multiplayer"
##      │  │     ├ MarginContainer       [v=3, ratio=0.05]
##      │  │     ├ ReplayButton          "Replays"   input_text="replay"
##      │  │     ├ MarginContainer2      [v=3]
##      │  │     └ MarginContainer3      ← 挂 VersionLabel.gd（字号 = 视口宽/60）
##      │  │        └ VersionLabel       "2.0.60"，右对齐（horizontal_alignment=2）
##      │  ├ MarginContainer5            [h=3, ratio=0.05]
##      │  └ MarginContainer6            [h=3, ratio=0.2]
##      └ AdContainer                    ← 运行时 0×0（AdContainer.gd 从 ad_size 取值）
##
## 六列的弹性比例之和 = 0.2 + 1 + 0.05 + 1 + 0.05 + 0.2 = **2.5**，于是（1920 宽、separation 4）：
##   左右留白 152 / **列 760** / 列间 38 / **列 760** / 38 / 152   （docs/124 §2.2 有实测表）
##
## 两个版本号事实（别混）：
##   * `.tscn` 里 `VersionLabel` 的静态文本是 `"0.0.0"`；
##   * `VersionLabel.gd` **只改字号**（= 视口宽/60），不动文本；
##   * 用户真机截图上显示的是 **`2.0.60`**，而 `project.binary` 的
##     `application/config/version` 正好是 `"2.0.60"` ⇒ 文本由运行时的 `GraphwarRootNode`
##     （Rust，封闭代码）从工程设置里填 —— **推断**，证据见 docs/124 §2.3。
##   所以这里照这个推断实现：读 `ProjectSettings` 的 `application/config/version`。

const GAME_TITLE := "Graphwar II"
# 字号除数（8 / 30 / 60）**不在本文件里**了：它们现在由原版脚本自己算
# （`client/src/views/TitleSize.gd` 等，逐字节副本，见 `docs/16` §40）。

## `.tscn` 静态文本（原版没填版本时的样子）。
const VERSION_PLACEHOLDER := "0.0.0"
const VERSION_SETTING := "application/config/version"

var _title: Label
var _version: Label

## 本屏的四个容器节点挂的是**原版脚本**（`client/src/views/`，逐字节副本），
## 所以 `font_divisor` / `apply_font_scale()` 在这里**不再需要**：
##   最外层 MarginContainer ← `MarginContainer.gd`（安全区内边距）
##   标题外层 MarginContainer ← `TitleSize.gd`（字号 = 宽/8，子节点最小宽 = 整屏宽）
##   两列 VBoxContainer ← `MainMenuButtonResize.gd`（字号 = 宽/30）
##   版本号外层 MarginContainer ← `VersionLabel.gd`（字号 = 宽/60）
func _ready() -> void:
	super()

func build() -> void:
	var outer := GwUI.margin("MarginContainer", 0)
	outer.name = "MarginContainer"
	outer.set_anchors_preset(Control.PRESET_FULL_RECT)
	outer.size_flags_vertical = Control.SIZE_EXPAND_FILL
	GwViews.attach(outer, "MarginContainer.gd")   # 原版 main_menu.tscn 的根节点就是它
	add_child(outer)
	var vbox := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, outer)

	# --- 标题（TitleSize.gd：子节点最小宽 = 整屏宽，字号 = 宽/8，EraserRegular）---
	var title_margin := GwUI.margin("MarginContainer", 0)
	title_margin.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_title = GwUI.label(GAME_TITLE, 0, HORIZONTAL_ALIGNMENT_CENTER)
	_title.name = "Label"
	_title.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	var title_font := GwThemeFactory.title_font()
	if title_font != null:
		_title.add_theme_font_override("font", title_font)
	title_margin.add_child(_title)
	# 填完子节点再挂原版脚本，最后才进树 —— `_ready` 只在进树时发一次，
	# 进树后再 `set_script()` 不会补发，原版脚本就永远连不上 `size_changed`。
	GwViews.attach(title_margin, "TitleSize.gd")
	vbox.add_child(title_margin)

	# --- 两列 ---
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, vbox)
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL

	var pad_left := _h_spacer("MarginContainer7", row, 0.2)

	var left_col_margin := GwUI.margin("MarginContainer", 0, row)
	left_col_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left_col_margin.size_flags_vertical = Control.SIZE_FILL
	var left_col := GwUI.vbox("VBoxContainer", GwUI.SEPARATION)
	left_col.size_flags_vertical = Control.SIZE_FILL
	var campaign := _menu_button("Campaign", GwCommands.CAMPAIGN, "PlayButton")
	left_col.add_child(campaign)
	_v_spacer("MarginContainer", left_col, 0.05)
	var player := _menu_button("Player\n", GwCommands.PLAYER_SETTINGS, "CharacterButton")
	left_col.add_child(player)
	_v_spacer("MarginContainer2", left_col, 1.0)
	GwViews.attach(left_col, "MainMenuButtonResize.gd")   # 字号 = 宽/30
	left_col_margin.add_child(left_col)

	var mid := _h_spacer("MarginContainer3", row, 0.05)

	var right_col_margin := GwUI.margin("MarginContainer2", 0, row)
	right_col_margin.name = "MarginContainer2"
	right_col_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right_col_margin.size_flags_vertical = Control.SIZE_FILL
	var right_col := GwUI.vbox("VBoxContainer2", GwUI.SEPARATION)
	right_col.size_flags_vertical = Control.SIZE_FILL
	var battle := _menu_button("Battle", GwCommands.MULTIPLAYER, "LocalButton")
	right_col.add_child(battle)
	_v_spacer("MarginContainer", right_col, 0.05)
	var replays := _menu_button("Replays", GwCommands.REPLAY, "ReplayButton")
	right_col.add_child(replays)
	_v_spacer("MarginContainer2", right_col, 1.0)
	GwViews.attach(right_col, "MainMenuButtonResize.gd")   # 字号 = 宽/30
	right_col_margin.add_child(right_col)

	# --- 版本号：右列**最下方**，右对齐（VersionLabel.gd 只管字号 = 宽/60）---
	var version_margin := GwUI.margin("MarginContainer3", 0)
	version_margin.name = "MarginContainer3"
	version_margin.size_flags_vertical = Control.SIZE_FILL
	_version = GwUI.label(version_text(), 0, HORIZONTAL_ALIGNMENT_RIGHT)
	_version.name = "VersionLabel"
	version_margin.add_child(_version)
	GwViews.attach(version_margin, "VersionLabel.gd")   # 字号 = 宽/60
	right_col.add_child(version_margin)

	var pad_right5 := _h_spacer("MarginContainer5", row, 0.05)
	var pad_right := _h_spacer("MarginContainer6", row, 0.2)

	# --- 广告位：节点在、尺寸 0、没有文字（原版 AdContainer.gd 把高度设成 ad_size=0）---
	vbox.add_child(GwUI.ad_slot())

## 版本号文本：`ProjectSettings` 的 `application/config/version`，缺省回落到 `.tscn` 的占位文本。
static func version_text() -> String:
	var v := String(ProjectSettings.get_setting(VERSION_SETTING, ""))
	return v if v != "" else VERSION_PLACEHOLDER

## 菜单按钮：**不**设 size_flags_vertical —— 原版 4 个按钮都不抢竖直空间，
## 竖直方向由两个 spacer 按 0.05 : 1 分配（我们以前给按钮写 EXPAND + custom_minimum_size=64，
## 于是 4 个按钮被拉得一样高、把整列塞满，与原版不符）。
func _menu_button(text_: String, cmd: String, node_name: String) -> GwButton:
	var b := GwButton.make(text_, cmd)
	b.name = node_name
	b.size_flags_vertical = Control.SIZE_FILL
	return b

## 竖直弹性空隙（原版：`MarginContainer` 标 `size_flags_vertical = 3` + stretch_ratio）。
func _v_spacer(node_name: String, parent: Node, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_vertical = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	return m

## 水平弹性空隙（原版：`MarginContainer` 标 `size_flags_horizontal = 3` + stretch_ratio）。
func _h_spacer(node_name: String, parent: Node, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	return m

func on_enter() -> void:
	# 版本号只取决于工程设置，不随状态变化；但路径保持与原版一致（每次进入刷新一次）。
	if _version != null:
		_version.text = version_text()
