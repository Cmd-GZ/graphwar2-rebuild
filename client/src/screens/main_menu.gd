extends GwScreen
## MainMenu —— 主菜单。
##
## **节点树、比例、字号全部来自原版 `.tscn`**：`client/scenes/screens/main_menu.tscn` 与
## `recovered-src/main_menu.tscn` **逐字节相同**（138 行 / 25 节点）。原版结构：
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
##      │  │        └ VersionLabel       "0.0.0"，右对齐（horizontal_alignment=2）
##      │  ├ MarginContainer5            [h=3, ratio=0.05]
##      │  └ MarginContainer6            [h=3, ratio=0.2]
##      └ AdContainer                    ← 运行时 0×0（AdContainer.gd 从 ad_size 取值）
##
## 六列的弹性比例之和 = 0.2 + 1 + 0.05 + 1 + 0.05 + 0.2 = **2.5**，于是（1920 宽、separation 4）：
##   左右留白 152 / **列 760** / 列间 38 / **列 760** / 38 / 152   （docs/124 §2.2 有实测表）
##
## 四个按钮是**原生 `GraphwarButton`**（`input_text` = Rust 侧命令键）：按下 → `input_selected`
## → `App.command`，接线在 `client/src/main.gd::_instantiate_screens()` 的 `GwButton.bind_native()`。
## 本文件**不再造任何节点** —— 手抄那棵 25 节点的树已随 `.tscn` 装上而删除（docs/06 §97）。
##
## 两个版本号事实（别混）：
##   * `.tscn` 里 `VersionLabel` 的静态文本是 `"0.0.0"`；
##   * `VersionLabel.gd` **只改字号**（= 视口宽/60），不动文本；
##   * 用户真机截图上显示的是 **`2.0.60`**，而 `project.binary` 的
##     `application/config/version` 正好是 `"2.0.60"` ⇒ 文本由运行时的 `GraphwarRootNode`
##     （Rust，封闭代码）从工程设置里填 —— **推断**，证据见 docs/124 §2.3。
##   所以这里照这个推断实现：读 `ProjectSettings` 的 `application/config/version`
##   （只是把 `.tscn` 的 `"0.0.0"` 占位文本换成工程版本号，不动节点树）。

## `.tscn` 静态文本（原版没填版本时的样子）。
const VERSION_PLACEHOLDER := "0.0.0"
const VERSION_SETTING := "application/config/version"

## `VersionLabel` 节点路径（原版 `.tscn` 里的路径，一字不改）。
const VERSION_LABEL_PATH := "MarginContainer/VBoxContainer/HBoxContainer/MarginContainer2"\
	+ "/VBoxContainer2/MarginContainer3/VersionLabel"

var _version: Label

## 字号除数**不在本文件里**：四个容器各挂原版脚本自己算
##   最外层 MarginContainer ← `MarginContainer.gd`（安全区内边距）
##   标题外层 MarginContainer ← `TitleSize.gd`（字号 = 宽/8，子节点最小宽 = 整屏宽）
##   两列 VBoxContainer ← `MainMenuButtonResize.gd`（字号 = 宽/30）
##   版本号外层 MarginContainer ← `VersionLabel.gd`（字号 = 宽/60）
## `font_divisor` 留在基类默认值上：`GwScreen._apply_size_recursive()` 见到
## `_is_view_owned()`（含 `res://<Name>.gd` 别名形态）就整棵跳过。
func _ready() -> void:
	super()

func build() -> void:
	_version = get_node_or_null(VERSION_LABEL_PATH) as Label

## 版本号文本：`ProjectSettings` 的 `application/config/version`，缺省回落到 `.tscn` 的占位文本。
static func version_text() -> String:
	var v := String(ProjectSettings.get_setting(VERSION_SETTING, ""))
	return v if v != "" else VERSION_PLACEHOLDER

func on_enter() -> void:
	# 版本号只取决于工程设置，不随状态变化；但路径保持与原版一致（每次进入刷新一次）。
	if _version != null:
		_version.text = version_text()
