extends RefCounted
class_name GwThemeFactory
## 主题与字体。
##
## 字体（`docs/121` §5.4）
## --------------------
## **不打包任何字体文件**，用使用者自己 `Graphwar.pck` 里的原版字体：
##
##   res://button_font.tres  → assets/button_font.res    （原版主字体，MSDF 已烘焙）
##   res://EraserRegular.ttf → assets/EraserRegular.fontdata（原版标题字体）
##
## 依据：恢复出的 `graphwar_theme.tres` 里 `Button/fonts/font = res://button_font.tres`
## （5 处主题都用它），`title_theme.tres` 的 `default_font = res://EraserRegular.ttf`。
## 包里**没有 `.ttf`**（只有 7 个 `.fontdata` 导入产物），所以这两条是"原版字体"的
## 唯一可得形态；找到就用，找不到就 `push_error` 并退回 Godot 内置字体
## （`GwAssets.missing()` 会记下来，selftest 会显式报告跳过）。
##
## 配色（`docs/121` §3）
## -------------------
## 不是我们挑的：全部抄自原版主题 `.tres` / `project.binary` 的**观测值**。
##
##   clear_color / boot_splash bg  = #0E251C  (0.054902, 0.145098, 0.109804)
##   Button/Panel normal           = #1A3B2D  bg + 3px 黑边 + 圆角 10
##   Button pressed                = #0F281D
##   Button disabled               = #383838
##
## 出处：`graphwar_style_box_flat.tres`、`graphwar_theme.tres`、
## `project.binary` 的 `rendering/environment/defaults/default_clear_color`。

## 基准字号。原版 `graphwar_theme.tres` 写的是 `default_font_size = 24`，但那是在
## **1600×900** 的视口里（原版 `project.binary` 的 viewport_width/height）；
## 本工程是 1280×720（`project.godot`），等物理大小换算 = 24 × 720/900 = 19.2 ≈ 20。
## 用 24 会让按钮/文字整体偏大、把战场挤到看不见（实测截图对比，见 docs/121 §7.2）。
const BASE_FONT_SIZE := 20

## 原版主字体 / 标题字体的 `res://` 路径（不含扩展名）。
const FONT_MAIN := "button_font"
const FONT_TITLE := "EraserRegular"

## --- 原版配色（观测值，不是设计选择）---
const COL_BG := Color(0.054902, 0.145098, 0.109804)          # #0E251C
const COL_PANEL := Color(0.101961, 0.231373, 0.176471)       # #1A3B2D
const COL_PANEL_EDGE := Color(0, 0, 0)                       # 原版边框恒为纯黑
const COL_PRESSED := Color(0.0588235, 0.156863, 0.113725)    # #0F281D
const COL_DISABLED := Color(0.219608, 0.219608, 0.219608)    # #383838
const COL_TEXT := Color(1, 1, 1)                             # Button/colors/font_color
const PANEL_RADIUS := 10
const PANEL_BORDER := 3

## --- 以下三项原版没有，是本客户端自用的（状态行 / 提示 / 队伍色）---
const COL_TEXT_DIM := Color(0.78, 0.78, 0.78)
const COL_ACCENT := Color(0.55, 0.85, 0.68)
const COL_WARN := Color(0.91, 0.65, 0.29)
const COL_BAD := Color(0.91, 0.45, 0.29)
const COL_TEAM1 := Color(0.31, 0.55, 0.88)
const COL_TEAM2 := Color(0.88, 0.45, 0.31)


## 缓存：`draw_string` 每次新建 FontFile 会拿不到已光栅化的字形（画出来是空框），
## 所以全局只用同一个 Font 对象（`docs/108` §9.2 坑 6）。
static var _font_cache: Font = null
static var _title_font_cache: Font = null


## 原版主字体。没有资产目录时返回 Godot 内置字体并报错（不静默）。
static func font() -> Font:
	if _font_cache == null:
		_font_cache = GwAssets.font(FONT_MAIN)
	return _font_cache


## 原版标题字体（`title_theme.tres`）。缺失时退回主字体。
static func title_font() -> Font:
	if _title_font_cache == null:
		_title_font_cache = GwAssets.font(FONT_TITLE)
		if _title_font_cache == null:
			_title_font_cache = font()
	return _title_font_cache


static func _flat(bg: Color, edge: Color = COL_PANEL_EDGE, radius: int = PANEL_RADIUS,
		width: int = PANEL_BORDER) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.border_color = edge
	sb.set_border_width_all(width)
	sb.set_corner_radius_all(radius)
	sb.content_margin_left = 12
	sb.content_margin_right = 12
	sb.content_margin_top = 6
	sb.content_margin_bottom = 6
	return sb


## 圆角 + 黑边 + 深绿底：原版 `graphwar_style_box_flat.tres` 的形状。
static func build() -> Theme:
	var t := Theme.new()
	t.default_font = font()
	t.default_font_size = BASE_FONT_SIZE

	# --- Button（原版 graphwar_theme.tres 的五个 stylebox）---
	t.set_stylebox("normal", "Button", _flat(COL_PANEL))
	t.set_stylebox("hover", "Button", _flat(COL_PANEL))
	t.set_stylebox("pressed", "Button", _flat(COL_PRESSED))
	t.set_stylebox("disabled", "Button", _flat(COL_DISABLED))
	t.set_stylebox("focus", "Button", StyleBoxEmpty.new())
	t.set_color("font_color", "Button", COL_TEXT)
	t.set_color("font_hover_color", "Button", COL_TEXT)
	t.set_color("font_pressed_color", "Button", COL_TEXT)
	t.set_color("font_disabled_color", "Button", COL_TEXT_DIM)
	t.set_font_size("font_size", "Button", BASE_FONT_SIZE)

	# --- Label ---
	t.set_color("font_color", "Label", COL_TEXT)
	t.set_font_size("font_size", "Label", BASE_FONT_SIZE)

	# --- LineEdit / TextEdit（原版：透明底 + 3px 黑边）---
	t.set_stylebox("normal", "LineEdit", _flat(Color(0.6, 0.6, 0.6, 0)))
	t.set_stylebox("focus", "LineEdit", _flat(Color(0.6, 0.6, 0.6, 0)))
	t.set_color("font_color", "LineEdit", COL_TEXT)
	t.set_color("caret_color", "LineEdit", COL_TEXT)
	t.set_font_size("font_size", "LineEdit", BASE_FONT_SIZE)
	t.set_stylebox("normal", "TextEdit", _flat(Color(0.6, 0.6, 0.6, 0)))
	t.set_color("font_color", "TextEdit", COL_TEXT)
	t.set_font_size("font_size", "TextEdit", BASE_FONT_SIZE)

	# --- Panel / PanelContainer（原版 graphwar_style_box_flat.tres）---
	t.set_stylebox("panel", "Panel", _flat(COL_PANEL))
	t.set_stylebox("panel", "PanelContainer", _flat(COL_PANEL))

	# --- TabContainer（PlayerScreen 的三个页签）---
	t.set_stylebox("panel", "TabContainer", _flat(COL_PANEL))
	t.set_stylebox("tab_selected", "TabContainer", _flat(COL_PANEL))
	t.set_stylebox("tab_unselected", "TabContainer", _flat(Color(0, 0, 0, 0)))
	t.set_color("font_selected_color", "TabContainer", COL_TEXT)
	t.set_color("font_unselected_color", "TabContainer", COL_TEXT_DIM)
	t.set_font_size("font_size", "TabContainer", BASE_FONT_SIZE)

	# --- RichTextLabel（聊天/函数列表/名字）---
	t.set_color("default_color", "RichTextLabel", COL_TEXT)
	t.set_font_size("normal_font_size", "RichTextLabel", 18)

	return t
